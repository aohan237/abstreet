// Copyright 2018 Google LLC, licensed under http://www.apache.org/licenses/LICENSE-2.0

use crate::screen_geom::ScreenRectangle;
use crate::{text, GfxCtx, ScreenPt, Text, UserInput};
use geom::{Bounds, Pt2D};
use graphics::Transformed;
use opengl_graphics::{Filter, GlyphCache, TextureSettings};
use std::cell::RefCell;

const ZOOM_SPEED: f64 = 0.1;

pub struct Canvas {
    // All of these f64's are in screen-space, so do NOT use Pt2D.
    // Public for saving/loading... should probably do better
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,

    // TODO We probably shouldn't even track screen-space cursor when we don't have the cursor.
    cursor_x: f64,
    cursor_y: f64,
    window_has_cursor: bool,

    left_mouse_drag_from: Option<ScreenPt>,

    pub window_width: f64,
    pub window_height: f64,

    glyphs: RefCell<GlyphCache<'static>>,

    // TODO Bit weird and hacky to mutate inside of draw() calls.
    covered_areas: RefCell<Vec<ScreenRectangle>>,
}

impl Canvas {
    pub fn new(initial_width: u32, initial_height: u32) -> Canvas {
        let texture_settings = TextureSettings::new().filter(Filter::Nearest);
        // TODO We could also preload everything and not need the RefCell.
        let glyphs = RefCell::new(
            GlyphCache::new(
                // TODO don't assume this exists!
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                (),
                texture_settings,
            )
            .expect("Could not load font"),
        );

        Canvas {
            cam_x: 0.0,
            cam_y: 0.0,
            cam_zoom: 1.0,

            cursor_x: 0.0,
            cursor_y: 0.0,
            window_has_cursor: true,

            left_mouse_drag_from: None,
            window_width: f64::from(initial_width),
            window_height: f64::from(initial_height),

            glyphs,

            covered_areas: RefCell::new(Vec::new()),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.left_mouse_drag_from.is_some()
    }

    pub fn handle_event(&mut self, input: &mut UserInput) {
        if let Some(pt) = input.get_moved_mouse() {
            self.cursor_x = pt.x;
            self.cursor_y = pt.y;

            if let Some(click) = self.left_mouse_drag_from {
                self.cam_x += click.x - pt.x;
                self.cam_y += click.y - pt.y;
                self.left_mouse_drag_from = Some(pt);
            }
        }
        // Can't start dragging on top of covered area
        if input.left_mouse_button_pressed() && self.get_cursor_in_map_space().is_some() {
            self.left_mouse_drag_from = Some(self.get_cursor_in_screen_space());
        }
        if input.left_mouse_button_released() {
            self.left_mouse_drag_from = None;
        }
        if let Some(scroll) = input.get_mouse_scroll() {
            // Zoom slower at low zooms, faster at high.
            let delta = scroll * ZOOM_SPEED * self.cam_zoom;
            self.zoom_towards_mouse(delta);
        }
        if input.window_gained_cursor() {
            self.window_has_cursor = true;
        }
        if input.window_lost_cursor() {
            self.window_has_cursor = false;
        }
    }

    pub(crate) fn start_drawing(&self, g: &mut GfxCtx) {
        g.ctx = g
            .orig_ctx
            .trans(-self.cam_x, -self.cam_y)
            .zoom(self.cam_zoom);
        self.covered_areas.borrow_mut().clear();
    }

    pub(crate) fn mark_covered_area(&self, rect: ScreenRectangle) {
        self.covered_areas.borrow_mut().push(rect);
    }

    pub fn draw_mouse_tooltip(&self, g: &mut GfxCtx, txt: Text) {
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let x1 = self.cursor_x - (width / 2.0);
        let y1 = self.cursor_y - (height / 2.0);
        // No need to cover the tooltip; this tooltip follows the mouse anyway.
        text::draw_text_bubble(g, glyphs, ScreenPt::new(x1, y1), txt);
    }

    // TODO Rename these draw_nonblocking_text_*
    pub fn draw_text_at(&self, g: &mut GfxCtx, txt: Text, map_pt: Pt2D) {
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let pt = self.map_to_screen(map_pt);
        text::draw_text_bubble(
            g,
            glyphs,
            ScreenPt::new(pt.x - (width / 2.0), pt.y - (height / 2.0)),
            txt,
        );
    }

    pub fn draw_text_at_topleft(&self, g: &mut GfxCtx, txt: Text, pt: Pt2D) {
        text::draw_text_bubble(
            g,
            &mut self.glyphs.borrow_mut(),
            self.map_to_screen(pt),
            txt,
        );
    }

    pub fn draw_text_at_screenspace_topleft(&self, g: &mut GfxCtx, txt: Text, pt: ScreenPt) {
        text::draw_text_bubble(g, &mut self.glyphs.borrow_mut(), pt, txt);
    }

    // The text box covers up what's beneath and eats the cursor (for get_cursor_in_map_space).
    pub fn draw_blocking_text(
        &self,
        g: &mut GfxCtx,
        txt: Text,
        (horiz, vert): (HorizontalAlignment, VerticalAlignment),
    ) {
        if txt.is_empty() {
            return;
        }
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let x1 = match horiz {
            HorizontalAlignment::Left => 0.0,
            HorizontalAlignment::Center => (self.window_width - width) / 2.0,
            HorizontalAlignment::Right => self.window_width - width,
        };
        let y1 = match vert {
            VerticalAlignment::Top => 0.0,
            VerticalAlignment::Center => (self.window_height - height) / 2.0,
            VerticalAlignment::Bottom => self.window_height - height,
        };
        self.covered_areas.borrow_mut().push(text::draw_text_bubble(
            g,
            glyphs,
            ScreenPt::new(x1, y1),
            txt,
        ));
    }

    pub(crate) fn text_dims(&self, txt: &Text) -> (f64, f64) {
        txt.dims(&mut self.glyphs.borrow_mut())
    }

    fn zoom_towards_mouse(&mut self, delta_zoom: f64) {
        let old_zoom = self.cam_zoom;
        self.cam_zoom += delta_zoom;
        if self.cam_zoom <= ZOOM_SPEED {
            self.cam_zoom = ZOOM_SPEED;
        }

        // Make screen_to_map of cursor_{x,y} still point to the same thing after zooming.
        self.cam_x = ((self.cam_zoom / old_zoom) * (self.cursor_x + self.cam_x)) - self.cursor_x;
        self.cam_y = ((self.cam_zoom / old_zoom) * (self.cursor_y + self.cam_y)) - self.cursor_y;
    }

    pub(crate) fn get_cursor_in_screen_space(&self) -> ScreenPt {
        ScreenPt::new(self.cursor_x, self.cursor_y)
    }

    pub fn get_cursor_in_map_space(&self) -> Option<Pt2D> {
        if self.window_has_cursor {
            let pt = self.get_cursor_in_screen_space();

            for rect in self.covered_areas.borrow().iter() {
                if rect.contains(pt) {
                    return None;
                }
            }

            Some(self.screen_to_map(pt))
        } else {
            None
        }
    }

    pub fn screen_to_map(&self, pt: ScreenPt) -> Pt2D {
        Pt2D::new(
            (pt.x + self.cam_x) / self.cam_zoom,
            (pt.y + self.cam_y) / self.cam_zoom,
        )
    }

    pub fn center_to_screen_pt(&self) -> ScreenPt {
        ScreenPt::new(self.window_width / 2.0, self.window_height / 2.0)
    }

    pub fn center_to_map_pt(&self) -> Pt2D {
        self.screen_to_map(self.center_to_screen_pt())
    }

    pub fn center_on_map_pt(&mut self, pt: Pt2D) {
        self.cam_x = (pt.x() * self.cam_zoom) - (self.window_width / 2.0);
        self.cam_y = (pt.y() * self.cam_zoom) - (self.window_height / 2.0);
    }

    fn map_to_screen(&self, pt: Pt2D) -> ScreenPt {
        ScreenPt::new(
            (pt.x() * self.cam_zoom) - self.cam_x,
            (pt.y() * self.cam_zoom) - self.cam_y,
        )
    }

    pub fn get_screen_bounds(&self) -> Bounds {
        let mut b = Bounds::new();
        b.update(self.screen_to_map(ScreenPt::new(0.0, 0.0)));
        b.update(self.screen_to_map(ScreenPt::new(self.window_width, self.window_height)));
        b
    }
}

pub enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

pub const BOTTOM_LEFT: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Left, VerticalAlignment::Bottom);
pub const TOP_RIGHT: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Right, VerticalAlignment::Top);
pub const CENTERED: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Center, VerticalAlignment::Center);
