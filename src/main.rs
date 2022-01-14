extern crate sdl2;

use std::net::{TcpStream};
use std::io::{Read, Write};
use std::io::{BufRead,BufReader};
use std::cmp::{min,max};
use std::time::Instant;

use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::surface::Surface;

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;
const BYTES_PER_PIXEL: usize = 3;
const BYTES_PER_FRAME: usize = WIDTH * HEIGHT * BYTES_PER_PIXEL;
const FULL_CROP: Crop = Crop { x: 0, y: 0, w: WIDTH as u16, h: HEIGHT as u16 };
const PIXEL_FORMAT: PixelFormatEnum = PixelFormatEnum::RGB24;

const WIN_WIDTH:  u32 = (WIDTH  / 2) as u32;
const WIN_HEIGHT: u32 = (HEIGHT / 2) as u32;

const ZOOM_FACTOR: f32 = 0.9;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct Crop {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

impl Into<Rect> for Crop {
    fn into(self) -> Rect {
        Rect::new(self.x as i32, self.y as i32, self.w as u32, self.h as u32)
    }
}

struct ChannelState {
    crop: Crop,
    full_rect: Rect,
    zoom_rect: Rect,
    server: TcpStream,
    frame: Vec<u8>,
    preview: Option<Rect>,
    undo: Option<Crop>,
}

impl ChannelState {
    fn set_crop(& mut self, mut nr: Rect) -> bool {
        let ox = if nr.left() < 0 { -nr.left() } else
            if nr.right() >= WIDTH as i32 { WIDTH as i32 - nr.right() - 1 }  else { 0 };
        let oy = if nr.top() < 0 { -nr.top() } else
            if nr.bottom() >= HEIGHT as i32 { HEIGHT as i32 - nr.bottom() - 1 }  else { 0 };
        nr.offset(ox, oy);

        let new_crop = Crop { x: max(0, nr.left()) as u16, y: max(0, nr.top()) as u16,
        w: min(WIDTH as u32, nr.width()) as u16, h: min(HEIGHT as u32, nr.height()) as u16 };
        if new_crop == self.crop { return false; }

        let line = self.text_cmd(format!("zoom_to {}x{}+{}+{}\n", new_crop.w, new_crop.h, new_crop.x, new_crop.y));
        if line == "OK\n" {
            self.undo = Some(self.crop);
            self.crop = new_crop;
            true
        } else {
            eprintln!("{line:?}");
            false
        }
    }

    fn text_cmd(& mut self, cmd: String) -> String {
        self.server.write_all(&cmd.into_bytes()).unwrap();
        let mut br = BufReader::new(&self.server);
        let mut line = String::new();
        br.read_line(&mut line).unwrap();
        line
    }
}

fn main() {
    let sdl = sdl2::init().unwrap();
    let video_subsystem = sdl.video().unwrap();
    let window = video_subsystem
        .window("Voctozoom", WIDTH as u32, HEIGHT as u32)
//        .resizable()
        .build()
        .unwrap();

    let mut state = [ChannelState {
        crop: FULL_CROP,
        full_rect: Rect::new(0, 0,                 WIN_WIDTH, WIN_HEIGHT),
        zoom_rect: Rect::new(0, WIN_HEIGHT as i32, WIN_WIDTH, WIN_HEIGHT),
        server: TcpStream::connect("127.0.0.1:20000").unwrap(),
        frame: vec![0; BYTES_PER_FRAME],
        preview: None,
        undo: None,
        // TODO add second channel
    }];

    sanity_check(& mut state);

    let mut canvas : Canvas<Window> = window.into_canvas()
        .present_vsync() //< this means the screen cannot
        // render faster than your display rate (usually 60Hz or 144Hz)
        .build().unwrap();

    let mut event_pump = sdl.event_pump().unwrap();
    let mut left_mouse_start_pos: Option<(i32, i32)> = None;
    let mut right_mouse_start_pos: Option<(i32, i32)> = None;
    let mut mouse_pos: (i32, i32) = (0, 0);

    let mut needs_update = true;
    let mut last_video = Instant::now();
    get_video(& mut state);

    'main: loop {
        if last_video.elapsed().as_millis() > 200 {
            last_video = Instant::now();
            get_video(& mut state);
            needs_update = true;
        }

        if needs_update {
            update_video(& mut canvas, & mut state);
            needs_update = false;
        }

        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => break 'main,
                sdl2::event::Event::MouseMotion { x, y, .. } => {
                    mouse_pos = (x, y);
                    if let Some(s) = left_mouse_start_pos {
                        'motion_left_states: for channel in &mut state {
                            let (p1, p2) = if channel.zoom_rect.contains_point(s) &&
                                    channel.zoom_rect.contains_point(mouse_pos) {
                                (mouse_pos, s)
                            }
                            else if channel.full_rect.contains_point(s) &&
                                    channel.full_rect.contains_point(mouse_pos) {
                                (s, mouse_pos)
                            }
                            else { continue; };

                            let mut nr: Rect = channel.crop.into();
                            let (dx, dy) = scale_point_from_window(p1, p2, WIDTH as i32, HEIGHT as i32, 0, 0);
                            nr.offset(dx, dy);
                            if channel.set_crop(nr) {
                                needs_update = true;
                            }
                            left_mouse_start_pos = Some(mouse_pos);
                            break 'motion_left_states;
                        }
                    }
                    if let Some(s) = right_mouse_start_pos {
                        'motion_right_states: for channel in &mut state {
                            if (channel.zoom_rect.contains_point(s) &&
                                    channel.zoom_rect.contains_point(mouse_pos)) ||
                               (channel.full_rect.contains_point(s) &&
                                    channel.full_rect.contains_point(mouse_pos)) {
                                channel.preview = Rect::from_enclose_points(
                                    &[Point::from(mouse_pos), Point::from(s)], None);
                                needs_update = true;
                                break 'motion_right_states;
                            }
                        }
                    }
                },
                sdl2::event::Event::MouseButtonDown { x, y, mouse_btn: MouseButton::Left, .. } => {
                    left_mouse_start_pos = Some((x, y));
                },
                sdl2::event::Event::MouseButtonDown { x, y, mouse_btn: MouseButton::Right, .. } => {
                    right_mouse_start_pos = Some((x, y));
                },
                sdl2::event::Event::MouseButtonUp { mouse_btn: MouseButton::Left, ..} => {
                    left_mouse_start_pos = None;
                },
                sdl2::event::Event::MouseButtonUp { mouse_btn: MouseButton::Right, ..} => {
                    right_mouse_start_pos = None;
                    'mouseup_states: for channel in &mut state {
                        if let Some(r) = channel.preview {
                            let (crop, frame) = if channel.zoom_rect.contains_rect(r) {
                                (channel.crop, channel.zoom_rect)
                            } else if channel.full_rect.contains_rect(r) {
                                (FULL_CROP, channel.full_rect)
                            } else { continue 'mouseup_states; };
                            let (x, y) = scale_point_from_window(
                                frame.top_left().into(), r.top_left().into(),
                                crop.w as i32, crop.h as i32, crop.x as i32, crop.y as i32);
                            let scaled_width  = r.width()  * crop.w as u32 / WIDTH  as u32;
                            let scaled_height = r.height() * crop.h as u32 / HEIGHT as u32;

                            let proposed_height = scaled_width * HEIGHT as u32 / WIDTH as u32;
                            let (nw, nh) = if proposed_height > scaled_height {
                                (scaled_width, proposed_height)
                            } else {
                                (scaled_height * WIDTH as u32 / HEIGHT as u32, scaled_height)
                            };

                            if channel.set_crop(Rect::from_center(
                                    (x + scaled_width as i32, y + scaled_height as i32),
                                    nw * 2, nh * 2)) {
                                needs_update = true;
                            }

                            channel.preview = None;
                            break 'mouseup_states;
                        }
                    }
                },
                sdl2::event::Event::MouseWheel { y, .. } => {
                    'wheel_states: for channel in &mut state {
                        if channel.zoom_rect.contains_point(mouse_pos) {
                            let r: Rect = channel.crop.into();
                            let p = scale_point_from_window(channel.zoom_rect.top_left().into(), mouse_pos,
                                r.width() as i32, r.height() as i32, r.left(), r.top());
                            let factor = if y > 0 { ZOOM_FACTOR } else { 1.0 / ZOOM_FACTOR };
                            let nw = (r.width()  as f32 * factor) as u32;
                            let nh = (r.height() as f32 * factor) as u32;
                            let nr = if nw >= WIDTH as u32 || nh >= HEIGHT as u32 { FULL_CROP.into() } else {
                                Rect::from_center(p, nw, nh)
                            };
                            if channel.set_crop(nr) {
                                needs_update = true;
                            }
                            break 'wheel_states;
                        }
                    }
                },
                sdl2::event::Event::KeyDown { keycode: Some(Keycode::Z), keymod, .. } => {
                    if !((keymod & (Mod::LCTRLMOD | Mod::RCTRLMOD)).is_empty()) {
                        'keydown_cz_states: for channel in &mut state {
                            if channel.zoom_rect.contains_point(mouse_pos) ||
                                    channel.full_rect.contains_point(mouse_pos) {
                                if let Some(c) = channel.undo {
                                    if channel.set_crop(c.into()) {
                                        channel.undo = None;
                                        needs_update = true;
                                    }
                                }
                                break 'keydown_cz_states
                            }
                        }
                    }
                }
                _ => {},
            }
        }
    }
}

fn scale_point_from_window(point: (i32, i32), offset: (i32, i32), width: i32, height: i32, left: i32, top: i32) -> (i32, i32) {
    let (x, y) = point;
    let (sx, sy) = offset;
    let dx = ((sx - x) *  width) / (WIN_WIDTH  as i32) + left;
    let dy = ((sy - y) * height) / (WIN_HEIGHT as i32) + top;
    (dx, dy)
}

fn sanity_check(state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        if channel.text_cmd(String::from("get_resolution\n")) != format!("{WIDTH}x{HEIGHT}\n") {
            panic!("Invalid resolution, must be {WIDTH}x{HEIGHT}");
        }
    }
}

fn get_video(state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        channel.server.write_all(b"get_image\n").unwrap();
        channel.server.read_exact(& mut channel.frame).unwrap();
    }
}

fn update_video(canvas: & mut Canvas<Window>, state: & mut [ChannelState]) {
    for channel in &mut state.iter_mut() {
        let surf = Surface::from_data(& mut channel.frame, WIDTH as u32, HEIGHT as u32, 3 * WIDTH as u32, PIXEL_FORMAT).unwrap();

        let tc = canvas.texture_creator();
        let tx = tc.create_texture_from_surface(surf).unwrap();

        canvas.copy(&tx, None, channel.full_rect).unwrap();

        let mut selected: Rect = channel.crop.into();

        let crop = if channel.crop == FULL_CROP { None } else { Some(selected) };

        canvas.copy(&tx, crop, channel.zoom_rect).unwrap();

        selected.set_width (selected.width()  / 2);
        selected.set_height(selected.height() / 2);
        selected.set_x       (selected.x()    / 2);
        selected.set_y       (selected.y()    / 2);
        selected.offset(channel.full_rect.left(), 0);

        canvas.set_draw_color(Color::RGB(255, 0, 0));
        canvas.draw_rect(selected).unwrap();

        if let Some(r) = channel.preview {
            canvas.set_draw_color(Color::RGB(0, 255, 0));
            canvas.draw_rect(r).unwrap();
        }
    }

    // XXX eprintln!("{:?}", canvas.output_size());

    // However the canvas has not been updated to the window yet,
    // everything has been processed to an internal buffer,
    // but if we want our buffer to be displayed on the window,
    // we need to call `present`. We need to call this everytime
    // we want to render a new frame on the window.
    canvas.present();
}
