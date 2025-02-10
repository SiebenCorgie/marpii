#![feature(proc_macro_diagnostic)]

use proc_macro::{Diagnostic, TokenStream};
use quote::{ToTokens, quote};
use syn::{
    Attribute, Data, DataStruct, DeriveInput, Expr, ExprLit, Field, Ident, Lit, parse_macro_input,
};

struct TaskAttribs {
    push_const: Option<Field>,
    push_handle: Vec<(Field, Ident)>,
}

impl TaskAttribs {
    fn from_data(data: &DataStruct) -> Self {
        let mut attribs = TaskAttribs {
            push_const: None,
            push_handle: Vec::with_capacity(0),
        };

        for field in data.fields.iter() {
            for attr in field.attrs.iter() {
                if attr.path().is_ident("push_constant") {
                    if attribs.push_const.is_some() {
                        Diagnostic::new(
                            proc_macro::Level::Error,
                            "PushConst can only be set once!",
                        )
                        .emit();
                    }

                    attribs.push_const = Some(field.clone());
                    continue;
                }

                if let Ok(kv) = attr.meta.require_name_value() {
                    if kv.path.is_ident("pc_handle") {
                        //map the expr to a field
                        let pcfield = match &kv.value {
                            Expr::Lit(ExprLit {
                                lit: Lit::Str(s), ..
                            }) => {
                                let ident: syn::Ident = s.parse().unwrap();
                                ident
                            }
                            _other => {
                                Diagnostic::new(
                                    proc_macro::Level::Error,
                                    format!(
                                        "Can not generate push constant field access from: {}",
                                        kv.value.to_token_stream()
                                    ),
                                )
                                .emit();
                                continue;
                            }
                        };

                        attribs.push_handle.push((field.clone(), pcfield));
                    }
                }
            }
        }

        attribs
    }
}

struct ComputePassArgs {
    source: Option<Expr>,
}

impl ComputePassArgs {
    fn from_attr(attr: Vec<Attribute>) -> Self {
        let mut source = None;

        for attr in attr {
            if let Ok(kv) = attr.meta.require_name_value() {
                if kv.path.is_ident("shader_source") {
                    source = Some(kv.value.clone());
                    continue;
                }
            }

            Diagnostic::new(
                proc_macro::Level::Error,
                format!("Unhandeled attribute: {}", attr.to_token_stream()),
            )
            .emit();
        }

        ComputePassArgs { source }
    }
}

///Derives a compute pipeline from the given shader-source.
///
/// Supported attributes:
///
/// - `push_constant`: signals that this is a push constant
/// - `pc_handle = xy`: signals that the tagged resource is referenced in the push constant as `xy`. Will automatically update the field `xy` of the `#[push_constant]` marked constant with the runtime [ResourceHandle](marpii_rmg_shared::ResourceHandle).
///
#[proc_macro_derive(TaskUtils, attributes(push_constant, pc_handle, shader_source))]
pub fn task_util_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let DeriveInput {
        attrs,
        vis: _,
        ident,
        generics: _,
        data,
    } = input;

    let cpa = ComputePassArgs::from_attr(attrs.clone());

    let attribs = match data {
        Data::Struct(dta) => {
            let attribs = TaskAttribs::from_data(&dta);
            attribs
        }
        _other => {
            Diagnostic::new(
                proc_macro::Level::Error,
                "Can only generate TaskUtils on structs!",
            )
            .emit();
            return quote! {}.into();
        }
    };

    let mut quote_stream = quote!();

    //Generates the packed source accessor
    if let Some(path) = cpa.source {
        quote_stream = quote! {
            #quote_stream
            impl #ident{
                ///Binary source code for the SPIR-V shader module.
                pub fn source() -> &'static [u8]{
                    include_bytes!(#path)
                }
            }
        }
    }

    //generates the pre-record push-constant overwrites
    if attribs.push_const.is_some() && attribs.push_handle.len() > 0 {
        let push_const_name = attribs.push_const.unwrap().ident.unwrap().clone();
        let fields = attribs.push_handle;

        let per_field_stream = fields
            .into_iter()
            .map(|(task_field, pcfield)| {
                let task_field = task_field.ident.unwrap().clone().to_token_stream();

                quote! {
                    self.#push_const_name.get_content_mut().#pcfield =
                        resources.resource_handle_or_bind(self.#task_field.clone())?
                }
            })
            .collect::<Vec<_>>();

        quote_stream = quote! {
            #quote_stream

            impl #ident{
                ///Writes all `#[pc_handle = "xy"]` marked resource handles to the `xy` field of the `#[push_constant]` marked field.
                pub fn write_resource_handle(&mut self, resources: &mut marpii_rmg::Resources) -> Result<(), marpii_rmg::RecordError>{
                    #(#per_field_stream;)*
                    Ok(())
                }
            }
        };
    } else {
        if attribs.push_const.is_none() && attribs.push_handle.len() > 0 {
            Diagnostic::new(
                proc_macro::Level::Warning,
                "#[pc_handle = \"..\"] was used, but no field was tagged with #[push_constant]",
            )
            .emit();
        }
    }

    quote_stream.into()
}
