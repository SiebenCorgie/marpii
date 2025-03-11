use std::hash::BuildHasherDefault;

use cosmic_text::{CacheKey, FontSystem, SwashCache};
use marpii::{ash::vk, resources::ImgDesc, util::ImageRegion};
use marpii_rmg::{ImageHandle, MetaTask, Rmg};
use marpii_rmg_tasks::DynamicImage;

pub struct GlyphEntry {
    pub content: cosmic_text::SwashContent,
    pub image_region: ImageRegion,
    pub data: Vec<u8>,
    pub placement: cosmic_text::Placement,
    //NOTE: the atlas allocation tends to be _bigger_ than the
    //image `image_region`. So we use _image_region_ to drive
    //copies, but use the atlas allocation for placement.
    pub atlas_allocation: etagere::Allocation,
}

impl GlyphEntry {
    fn image_region(&self) -> ImageRegion {
        self.image_region
    }
}

struct GlyphUpload {
    region: ImageRegion,
    data: Vec<u8>,
}

struct AtlasCache {
    atlas_texture: DynamicImage,
    atlas: etagere::BucketedAtlasAllocator,
    is_new_image: bool,
    uploads: Vec<GlyphUpload>,
}

impl AtlasCache {
    const INITAL_SIZE: i32 = 512;
    fn new(rmg: &mut Rmg, format: vk::Format, name: &str) -> Self {
        let atlas_texture = DynamicImage::new(
            rmg,
            ImgDesc::texture_2d(Self::INITAL_SIZE as u32, Self::INITAL_SIZE as u32, format),
            Some(name),
        )
        .unwrap();

        AtlasCache {
            atlas_texture,
            atlas: etagere::BucketedAtlasAllocator::new(etagere::size2(
                Self::INITAL_SIZE,
                Self::INITAL_SIZE,
            )),
            is_new_image: true,
            uploads: Vec::new(),
        }
    }

    fn free(&mut self, id: etagere::AllocId) {
        self.atlas.deallocate(id);
    }
}

pub struct FontAtlasCache {
    //Rasterization cache.
    swash_cache: SwashCache,
    color_cache: AtlasCache,
    mask_cache: AtlasCache,

    ///The LRU maps a GlyphCache key to the atlas allocator.
    lru: lru::LruCache<CacheKey, GlyphEntry, Hasher>,
}

type Hasher = BuildHasherDefault<ahash::AHasher>;

impl MetaTask for FontAtlasCache {
    fn record<'a>(
        &'a mut self,
        recorder: marpii_rmg::Recorder<'a>,
    ) -> Result<marpii_rmg::Recorder<'a>, marpii_rmg::RecordError> {
        //for both caches, record all residing glyphs as upload, if they are _new_

        if self.color_cache.is_new_image {
            self.color_cache.is_new_image = false;
            let mut desc = self.color_cache.atlas_texture.image.image_desc().clone();
            desc.extent.width = self.color_cache.atlas.size().width as u32;
            desc.extent.height = self.color_cache.atlas.size().height as u32;

            //update the image
            self.color_cache.atlas_texture =
                DynamicImage::new(recorder.rmg, desc, Some("AtlasColor")).unwrap();

            for (_k, v) in self.lru.iter() {
                //push all content that is color or subpixel-mask to the color cache
                if v.content == cosmic_text::SwashContent::Color
                    || v.content == cosmic_text::SwashContent::SubpixelMask
                {
                    self.color_cache
                        .atlas_texture
                        .write_bytes(recorder.rmg, v.image_region(), &v.data)
                        .unwrap();
                }
            }
        }
        //same for masks
        if self.mask_cache.is_new_image {
            self.mask_cache.is_new_image = false;
            let mut desc = self.mask_cache.atlas_texture.image.image_desc().clone();
            desc.extent.width = self.mask_cache.atlas.size().width as u32;
            desc.extent.height = self.mask_cache.atlas.size().height as u32;
            //update the image
            self.mask_cache.atlas_texture =
                DynamicImage::new(recorder.rmg, desc, Some("AtlasMask")).unwrap();

            for (_k, v) in self.lru.iter() {
                //push all content that is color or subpixel-mask to the color cache
                if v.content == cosmic_text::SwashContent::Mask {
                    self.mask_cache
                        .atlas_texture
                        .write_bytes(recorder.rmg, v.image_region(), &v.data)
                        .unwrap();
                }
            }
        }

        //now handle all uploads
        for upload in &self.color_cache.uploads {
            self.color_cache
                .atlas_texture
                .write_bytes(recorder.rmg, upload.region, &upload.data)
                .unwrap();
        }
        self.color_cache.uploads.clear();
        for upload in &self.mask_cache.uploads {
            self.mask_cache
                .atlas_texture
                .write_bytes(recorder.rmg, upload.region, &upload.data)
                .unwrap();
        }
        self.mask_cache.uploads.clear();

        //now handle both in the recorder
        recorder
            .add_task(&mut self.color_cache.atlas_texture)
            .unwrap()
            .add_task(&mut self.mask_cache.atlas_texture)
    }
}

impl FontAtlasCache {
    ///The LRU element count that is desired.
    //TODO: That should probably be tuneable.
    const LRU_TRIM_SIZE: usize = 1024;

    pub fn new(rmg: &mut Rmg) -> Self {
        let color_cache = AtlasCache::new(rmg, vk::Format::R8G8B8A8_UNORM, "AtlasColor");
        let mask_cache = AtlasCache::new(rmg, vk::Format::R8_UNORM, "AtlasMask");

        FontAtlasCache {
            color_cache,
            mask_cache,
            swash_cache: SwashCache::new(),
            lru: lru::LruCache::unbounded_with_hasher(Hasher::default()),
        }
    }

    pub fn trim(&mut self) {
        //trim both caches by removing elements from the LRU, and deallocating them from
        //the correct cache

        while self.lru.len() > Self::LRU_TRIM_SIZE {
            let (_glyph_key, glyph) = self.lru.pop_lru().unwrap();
            log::warn!("Deallocating glyph");
            match glyph.content {
                cosmic_text::SwashContent::Mask => self.mask_cache.free(glyph.atlas_allocation.id),
                cosmic_text::SwashContent::SubpixelMask | cosmic_text::SwashContent::Color => {
                    self.color_cache.free(glyph.atlas_allocation.id)
                }
            }
        }
    }

    pub fn find_or_create_glyph(
        &mut self,
        glyph: CacheKey,
        font_system: &mut FontSystem,
    ) -> Option<&GlyphEntry> {
        let should_promote = if let Some(_cached) = self.lru.get(&glyph) {
            true
        } else {
            //no such glyph yet, rasterize one, then insert

            let Some(image) = self.swash_cache.get_image(font_system, glyph) else {
                log::error!("Could not rasterize glyph!");
                return None;
            };

            if image.placement.width == 0 || image.placement.height == 0 {
                return None;
            }

            //select the atlas, based on the content type
            let cache = match image.content {
                cosmic_text::SwashContent::Mask => &mut self.mask_cache,
                cosmic_text::SwashContent::Color | cosmic_text::SwashContent::SubpixelMask => {
                    &mut self.color_cache
                }
            };

            let alloc = loop {
                match cache.atlas.allocate(etagere::size2(
                    image.placement.width as i32,
                    image.placement.height as i32,
                )) {
                    Some(alloc) => {
                        break alloc;
                    }
                    None => {
                        //the new groth size is the next power-of-two texture size
                        let cwidth = cache.atlas.size().width;
                        let cheight = cache.atlas.size().height;
                        //NOTE: we always grow all,
                        //TODO: at some point we might have to change that. But combined with eager trimming
                        //      that _should_ be okey
                        let nwidth = cwidth * 2;
                        let nheight = cheight * 2;

                        cache.is_new_image = true;

                        log::info!(
                            "Growing to {nwidth}/{nheight} for {}/{}",
                            image.placement.width,
                            image.placement.height
                        );

                        if nwidth >= 4096 || nheight >= 4096 {
                            log::warn!("Approaching thick glyph cache: {nwidth}x{nheight}");
                        }

                        cache.atlas.grow(etagere::size2(nwidth, nheight));
                    }
                }
            };

            let image_region = ImageRegion {
                offset: vk::Offset3D {
                    x: alloc.rectangle.min.x,
                    y: alloc.rectangle.min.y,
                    z: 0,
                },
                extent: vk::Extent3D {
                    width: image.placement.width,
                    height: image.placement.height,
                    depth: 1,
                },
            };
            let glyph_entry = GlyphEntry {
                atlas_allocation: alloc,
                image_region,
                data: image.data.clone(),
                content: image.content,
                placement: image.placement,
            };

            //put the glyph into the lru
            self.lru.put(glyph, glyph_entry);
            cache.uploads.push(GlyphUpload {
                region: image_region,
                data: image.data.clone(),
            });

            false
        };

        if should_promote {
            self.lru.promote(&glyph);
        }
        self.lru.get(&glyph)
    }

    pub fn glyph_texture_color(&self) -> ImageHandle {
        self.color_cache.atlas_texture.image.clone()
    }
    pub fn glyph_texture_alpha(&self) -> ImageHandle {
        self.mask_cache.atlas_texture.image.clone()
    }
}

pub fn glyph_content_to_type(content: cosmic_text::SwashContent) -> u32 {
    match content {
        cosmic_text::SwashContent::Mask => 0,
        cosmic_text::SwashContent::SubpixelMask => 1,
        cosmic_text::SwashContent::Color => 2,
    }
}
