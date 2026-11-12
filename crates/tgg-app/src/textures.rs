//! A DAT read as a document to edit: its textures, where a stock fighter
//! draws each, and what each is called ("Head #14", a stage's own names, or
//! "Texture 3").

use dat_edit::{DocumentError, DocumentTexture, TextureDocument};
use melee_dat::MeleeReferenceStore;
use melee_dat::fighter::places::{CostumePlaces, TexturePlace};
use melee_dat::stage::StageTextureNames;

use crate::References;

pub struct Listing {
    pub document: Result<TextureDocument, DocumentError>,
    /// Where a stock fighter draws each texture, when that's known.
    pub places: Vec<Option<TexturePlace>>,
    /// Each texture's name, in document order.
    pub names: Vec<String>,
}

impl Listing {
    /// Read `dat`. A stock costume's `store` names its textures by where the
    /// fighter draws them; a stock stage names its own.
    pub fn read(
        dat: Vec<u8>,
        references: Option<&References>,
        store: Option<&MeleeReferenceStore>,
    ) -> Self {
        let references = references.zip(store);
        // Places come from the same parse as the document.
        let opened = TextureDocument::open_inspecting(dat, |dat, scene| {
            let places = references.map(|(references, store)| {
                CostumePlaces::read(dat, scene, references.catalog, store)
            });
            (places, StageTextureNames::read(dat))
        });
        let (document, places, stage_names) = match opened {
            Ok((document, (places, stage_names))) => (document, places, stage_names),
            Err(error) => {
                return Self {
                    document: Err(error),
                    places: Vec::new(),
                    names: Vec::new(),
                };
            }
        };
        let places = match places {
            Some(Ok(places)) => places,
            Some(Err(error)) => {
                crate::log(&format!("texture names unavailable: {error}"));
                None
            }
            None => None,
        };
        let textures = document.textures();
        let places: Vec<Option<TexturePlace>> = textures
            .iter()
            .map(|texture| places.as_ref()?.place_of(&texture.descriptors()))
            .collect();
        let stage_name = |texture: &DocumentTexture| stage_names.name_of(texture.data_offset);
        let names = textures
            .iter()
            .zip(&places)
            .enumerate()
            .map(|(index, (texture, place))| match stage_name(texture) {
                Some(name) => name.to_owned(),
                None => texture_name(index, *place),
            })
            .collect();
        Self {
            document: Ok(document),
            places,
            names,
        }
    }

    /// Texture `index`'s name.
    pub fn name(&self, index: usize) -> String {
        self.names
            .get(index)
            .cloned()
            .unwrap_or_else(|| texture_name(index, None))
    }
}

/// A texture's name: where a stock costume draws it and its number, or just
/// the number.
pub fn texture_name(texture: usize, place: Option<TexturePlace>) -> String {
    match place {
        Some(place) => format!("{} #{}", place.label(), texture + 1),
        None => format!("Texture {}", texture + 1),
    }
}

/// A document texture's (width, height), which its decoded pixels have.
pub fn texture_size(texture: &DocumentTexture) -> (u32, u32) {
    (texture.width.into(), texture.height.into())
}

/// What a texture is, in a line: its animation frame, size, format and how
/// many times the model draws it ("frame 2/8 · 64×64 · CMPR · 3 uses").
pub fn texture_detail(texture: &DocumentTexture) -> String {
    let frame = texture.frame.as_ref().map_or_else(String::new, |frame| {
        format!("frame {}/{} · ", frame.frame + 1, frame.frames)
    });
    let uses = match texture.uses.len() {
        1 => String::new(),
        uses => format!(" · {uses} uses"),
    };
    format!(
        "{frame}{}×{} · {}{uses}",
        texture.width,
        texture.height,
        texture.format.name()
    )
}
