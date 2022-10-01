use easy_gltf::Scene;
use shared::Vertex;

pub fn load_model(gltf: &[Scene]) -> (Vec<Vertex>, Vec<u32>) {


    let mut vertex_buffer = Vec::new();
    let mut index_buffer = Vec::new();

    for scene in gltf {
        println!("Loading scene");

        for model in &scene.models {
            println!("Loading mesh with {} verts", model.vertices().len());

            /*
            let texture_sampler = Arc::new(Sampler::new(
                &app.ctx.device,
                &vk::SamplerCreateInfo::builder()
                    .mipmap_mode(SamplerMipmapMode::LINEAR)
            ).unwrap());

            //Load albedo texture
            let albedo: ImageBuffer<Rgba<f32>, Vec<f32>> = DynamicImage::from(model.material().pbr.base_color_texture.as_ref().unwrap().deref().clone()).into_rgba32f();
            let albedo_texture = Arc::new(image_from_image(
                &app.ctx.device,
                &app.ctx.allocator,
                app.ctx.device.first_queue_for_attribute(true, false, false).unwrap(),
                vk::ImageUsageFlags::SAMPLED,
                marpii_commands::image::DynamicImage::from(albedo),
            ).unwrap());

            let albedo_view = Arc::new(albedo_texture.view(&app.ctx.device, albedo_texture.view_all()).unwrap());

            let albedo_handle = if let Ok(hdl) = app.bindless.bindless_descriptor.bind_sampled_image(
                albedo_view,
                texture_sampler.clone()
            ){
                hdl
            }else{
                panic!("Couldn't bind!")
            };

            */


            let index_offset = vertex_buffer.len() as u32;
            for v in model.vertices(){
                vertex_buffer.push(Vertex{
                    position: v.position.into(),
                    normal: v.normal.into(),
                    uv: v.tex_coords.into()
                });
            }

            for i in model.indices().expect("Mesh has no index buffer"){
                index_buffer.push(index_offset + *i as u32);
            }
        }
    }

    (vertex_buffer, index_buffer)
}
