//! The editor on a real fighter, driven through gpui's test app: what only a
//! stock costume with its fighter's files can show (named textures, eye
//! frames, surfaces drawn with two textures, moves). Flows that any model
//! exercises are tested on a hand-written one in `editor::flows`.
//!
//! They read a clean Melee NTSC 1.02 disc image named by `TGG_MELEE_ISO`:
//!
//! ```text
//! TGG_MELEE_ISO=/path/to/melee.iso cargo test -p tgg-editor --features melee-iso
//! ```

use crate::editor::Editor;
use crate::open_file::OpenFile;
use crate::settings::Settings;
use crate::{Game, game_references};
use gpui::{TestAppContext, WindowHandle};
use std::rc::Rc;
use tgg_ui::{Appearance, OptionRow, Theme};

/// Open Falco Red from the disc in an editor window, as the app does.
fn open_falco(cx: &mut TestAppContext) -> WindowHandle<Editor> {
    let iso = std::env::var_os("TGG_MELEE_ISO")
        .expect("set TGG_MELEE_ISO to a clean Melee NTSC 1.02 disc image");
    let game = Game::open(iso.as_ref()).expect("open the disc image");
    let dat = game.read("PlFcRe.dat").expect("read Falco Red");
    let references = Rc::new(game_references(game).expect("references"));
    cx.update(|cx| {
        Theme::init(Appearance::Gallery, cx);
        tgg_ui::init(cx);
    });
    cx.add_window(move |_, cx| {
        let file =
            OpenFile::show("PlFcRe.dat", dat, Some(&references), cx).expect("load Falco Red");
        Editor::new(file, Some(references), Settings::default(), None, cx)
    })
}

fn notice(editor: &Editor) -> String {
    editor
        .notice
        .as_ref()
        .map_or_else(String::new, |notice| notice.text.to_string())
}

#[gpui::test]
fn opening_names_every_texture_and_selects_the_first(cx: &mut TestAppContext) {
    let window = open_falco(cx);
    window
        .update(cx, |editor, _, _| {
            let document = editor.document.as_ref().expect("the document opens");
            assert_eq!(document.textures().len(), 27);
            assert_eq!(editor.selected(), Some(0));
            // Eye frames are listed, and the eyes are named.
            assert_eq!(editor.texture_name(15), "Eyes #16");
            assert!(document.textures()[25].frame.is_some());
            assert!(!editor.is_modified());
        })
        .unwrap();
}

#[gpui::test]
fn clicking_a_surface_again_steps_through_its_textures_then_unselects(cx: &mut TestAppContext) {
    let window = open_falco(cx);
    window
        .update(cx, |editor, _, cx| {
            // A click on a surface drawn with two textures reports both, most
            // defining first: here, textures 12 and 14 by their drawn uses.
            let uses = |texture: usize| {
                editor.document.as_ref().unwrap().textures()[texture]
                    .uses
                    .clone()
            };
            let surface = [uses(11), uses(13)];
            editor.select(None, cx);
            editor.select_picked(&surface, cx);
            assert_eq!(editor.selected(), Some(11));
            editor.select_picked(&surface, cx);
            assert_eq!(editor.selected(), Some(13));
            editor.select_picked(&surface, cx);
            assert_eq!(editor.selected(), None);
            // The background selects nothing and says nothing.
            editor.select_picked(&[], cx);
            assert_eq!(editor.selected(), None);
        })
        .unwrap();
}

/// The move list's row titled `title`.
fn move_row(editor: &Editor, title: &str) -> usize {
    editor
        .moves
        .rows
        .iter()
        .position(|row| matches!(row, OptionRow::Option { title: listed, .. } if listed == title))
        .unwrap_or_else(|| panic!("no move titled {title}"))
}

#[gpui::test]
fn choosing_a_move_plays_it_and_a_refused_one_says_why(cx: &mut TestAppContext) {
    let window = open_falco(cx);
    window
        .update(cx, |editor, _, cx| {
            let playing = |editor: &Editor, cx: &mut gpui::Context<Editor>| {
                let viewport = editor.viewport.read(cx);
                viewport
                    .playback()
                    .map(|playback| playback.label().to_owned())
            };
            assert_eq!(playing(editor, cx).as_deref(), Some("Falco Wait1"));
            assert!(editor.moves.playable > 200, "{}", editor.moves.playable);

            editor.play_move(move_row(editor, "Jab 1"), cx);
            assert_eq!(playing(editor, cx).as_deref(), Some("Falco Attack11"));
            assert!(editor.notice.is_none());

            // Back throw is listed but refused; the jab keeps playing.
            let back_throw = move_row(editor, "Back throw");
            let OptionRow::Option { disabled, .. } = &editor.moves.rows[back_throw] else {
                unreachable!()
            };
            assert!(
                disabled
                    .as_ref()
                    .is_some_and(|reason| reason.contains("part of the body"))
            );
            editor.play_move(back_throw, cx);
            assert_eq!(playing(editor, cx).as_deref(), Some("Falco Attack11"));
            assert!(
                notice(editor).starts_with("Can't play that move"),
                "{}",
                notice(editor)
            );
        })
        .unwrap();
}

#[gpui::test]
fn scrubbing_pauses_on_the_frame_and_speed_carries_to_the_next_move(cx: &mut TestAppContext) {
    let window = open_falco(cx);
    window
        .update(cx, |editor, _, cx| {
            editor.play_move(move_row(editor, "Up smash"), cx);
            editor.viewport.update(cx, |viewport, cx| {
                viewport.seek(12.0, cx).expect("seek");
                assert!(viewport.is_paused());
                assert_eq!(viewport.playback().unwrap().frame(), 12.0);
                viewport.set_rate(0.5, cx).expect("rate");
            });
            // Choosing a move plays it from its start at the chosen speed.
            editor.play_move(move_row(editor, "Down smash"), cx);
            let viewport = editor.viewport.read(cx);
            assert!(!viewport.is_paused());
            let playback = viewport.playback().unwrap();
            assert_eq!((playback.frame(), playback.rate()), (0.0, 0.5));
        })
        .unwrap();
}
