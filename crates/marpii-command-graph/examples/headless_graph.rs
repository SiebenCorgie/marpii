use marpii::{
    allocator::Allocator,
    ash::vk,
    context::Ctx,
    resources::{Image, ImgDesc},
};
use marpii_command_graph::{
    pass::{AssumedState, Pass},
    Graph, ImageState, StImage,
};

struct DummyPass {
    assumed: Vec<AssumedState>,
}
impl Pass for DummyPass {
    fn assumed_states(&self) -> &[marpii_command_graph::pass::AssumedState] {
        &self.assumed
    }
    fn record(
        &mut self,
        command_buffer: &mut marpii_commands::Recorder,
    ) -> Result<(), anyhow::Error> {
        println!("RECORD");
        Ok(())
    }
    fn requirements(&self) -> &'static [marpii_command_graph::pass::SubPassRequirement] {
        &[]
    }
}

fn simple_image<A: Allocator + Send + Sync + 'static>(ctx: &Ctx<A>, name: &str) -> StImage {
    StImage::unitialized(
        Image::new(
            &ctx.device,
            &ctx.allocator,
            ImgDesc::default(),
            marpii::allocator::MemoryUsage::GpuOnly,
            Some(name),
            None,
        )
        .unwrap(),
    )
}

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    let ctx = Ctx::new_headless(true).unwrap();

    let gbuffer = simple_image(&ctx, "Gbuffer");
    let shadow = simple_image(&ctx, "Shadow");
    let light = simple_image(&ctx, "Light");
    let post = simple_image(&ctx, "Post");

    let mut graph = Graph::new(&ctx.device)
        .insert_pass(
            "Gbuffer",
            DummyPass {
                assumed: vec![AssumedState::Image {
                    image: gbuffer.clone(),
                    state: ImageState {
                        access_mask: vk::AccessFlags::SHADER_WRITE,
                        layout: vk::ImageLayout::GENERAL,
                    },
                }],
            },
            0,
        )
        .insert_pass(
            "AsyncShadow",
            DummyPass {
                assumed: vec![AssumedState::Image {
                    image: shadow.clone(),
                    state: ImageState {
                        access_mask: vk::AccessFlags::SHADER_WRITE,
                        layout: vk::ImageLayout::GENERAL,
                    },
                }],
            },
            1,
        )
        .insert_pass(
            "Light",
            DummyPass {
                assumed: vec![
                    AssumedState::Image {
                        image: shadow.clone(),
                        state: ImageState {
                            access_mask: vk::AccessFlags::SHADER_READ,
                            layout: vk::ImageLayout::GENERAL,
                        },
                    },
                    AssumedState::Image {
                        image: gbuffer.clone(),
                        state: ImageState {
                            access_mask: vk::AccessFlags::SHADER_READ,
                            layout: vk::ImageLayout::GENERAL,
                        },
                    },
                    AssumedState::Image {
                        image: light.clone(),
                        state: ImageState {
                            access_mask: vk::AccessFlags::SHADER_READ,
                            layout: vk::ImageLayout::GENERAL,
                        },
                    },
                ],
            },
            0,
        )
        .insert_pass(
            "Post",
            DummyPass {
                assumed: vec![
                    AssumedState::Image {
                        image: light.clone(),
                        state: ImageState {
                            access_mask: vk::AccessFlags::SHADER_READ,
                            layout: vk::ImageLayout::GENERAL,
                        },
                    },
                    AssumedState::Image {
                        image: post.clone(),
                        state: ImageState {
                            access_mask: vk::AccessFlags::SHADER_WRITE,
                            layout: vk::ImageLayout::GENERAL,
                        },
                    },
                ],
            },
            0,
        )
        .insert_pass(
            "Present",
            DummyPass {
                assumed: vec![AssumedState::Image {
                    image: post.clone(),
                    state: ImageState {
                        access_mask: vk::AccessFlags::empty(),
                        layout: vk::ImageLayout::PRESENT_SRC_KHR,
                    },
                }],
            },
            0,
        )
        .build()
        .unwrap();

    println!("Blub");

    graph.submit().unwrap();
    println!("Bye bye");
}
