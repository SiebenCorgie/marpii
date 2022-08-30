use crate::resources::ImageKey;

use super::{Task, Attachment, AttachmentType};



pub struct Blit{
    src_image: ImageKey,
    dst_image: ImageKey
}

impl Task for Blit {
    fn attachments(&self) -> &[Attachment] {
        &[]
    }

    fn images(&self) -> &[ImageKey] {
        //TODO: Find a way to use inbuild copy blit operation, or rewrite?
        &[]
    }

    fn record(&self, recorder: &mut crate::graph::TaskRecord) {
        todo!("Blit unimplemented");
    }
}
