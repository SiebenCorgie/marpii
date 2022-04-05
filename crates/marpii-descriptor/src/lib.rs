//! # Managed Descriptor
//!
//! Similar to marpii-commands this crate provides highlever abstractions for descriptorsets
//!
//! Whenever a image, buffer or sampler is bound to a descriptor the pipeline using this descriptor assumes that the
//! resource is alive when being used. Therefore the programmer has to ensure proper lifetime of such a resource.
//!
//! This crate solves this lifetime tracking by bundling a descriptor set with all resources that are needed to make the set
//! valid.
//!
//! Apart from that a self growing descriptor pool implementation is provided that removes the need for proper
//! pre-allocation of descriptors ina  pool. It also ensures that unneeded descriptors are recycled.
//!
//! Another helper is the bindless helper, that lets you build a bindless-like descriptor set with proper resource
//! allocation and lifetime handling. Have a look at its module documentation.
//!

///Bindless descriptor handling related structures.
pub mod bindless;

pub mod dynamic_pool;

pub mod managed_descriptor;
