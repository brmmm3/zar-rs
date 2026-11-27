use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ClipOrdering, ConfigureWindowAux, ConnectionExt,
    CreateGCAux, CreateWindowAux, EventMask, KeyButMask, PropMode, SelectionNotifyEvent, StackMode,
    Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        XdndAware,
        XdndEnter,
        XdndPosition,
        XdndStatus,
        XdndLeave,
        XdndDrop,
        XdndFinished,
        XdndSelection,
        XdndActionCopy,
        TARGETS,
        text_uri_list: b"text/uri-list",
    }
}

pub struct X11Drag {
    result: Option<Receiver<Result<(), String>>>,
    preview_cancel: Option<Arc<AtomicBool>>,
    origin: Window,
}

impl X11Drag {
    pub fn new(origin: Window) -> Self {
        Self {
            result: None,
            preview_cancel: None,
            origin,
        }
    }

    pub fn show_preview(
        &mut self,
        paths: &[PathBuf],
        icon: crate::ui::UiIcon,
        theme: crate::settings::IconTheme,
        scale: f32,
    ) {
        if let Some(cancel) = self.preview_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let label = preview_label(paths);
        let icon = crate::ui::icon_bytes(icon, theme);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            if let Err(error) = run_preview(None, &label, icon, scale, &worker_cancel) {
                eprintln!("zarg: could not show X11 drag preview: {error}");
            }
        });
        self.preview_cancel = Some(cancel);
    }

    pub fn active(&self) -> bool {
        self.result.is_some()
    }

    pub fn start(
        &mut self,
        paths: &[PathBuf],
        context: &eframe::egui::Context,
    ) -> Result<(), String> {
        self.start_on_display(paths, context, None)
    }

    fn start_on_display(
        &mut self,
        paths: &[PathBuf],
        context: &eframe::egui::Context,
        display: Option<String>,
    ) -> Result<(), String> {
        if self.active() {
            return Err("An X11 drag is already in progress".into());
        }
        let data = crate::wayland_drag::uri_list(paths)?;
        let (sender, receiver) = mpsc::channel();
        let context = context.clone();
        let origin = self.origin;
        std::thread::spawn(move || {
            let result =
                run_drag(data, display.as_deref(), origin).map_err(|error| error.to_string());
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.result = Some(receiver);
        Ok(())
    }

    pub fn poll(&mut self) -> Result<(), String> {
        if let Some(receiver) = &self.result {
            match receiver.try_recv() {
                Ok(result) => {
                    self.result = None;
                    return result;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.result = None;
                    return Err("The X11 drag worker stopped unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        Ok(())
    }
}

impl Drop for X11Drag {
    fn drop(&mut self) {
        if let Some(cancel) = self.preview_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

type X11Result<T> = Result<T, Box<dyn std::error::Error>>;

fn run_preview(
    display: Option<&str>,
    label: &str,
    icon: &[u8],
    scale: f32,
    cancel: &AtomicBool,
) -> X11Result<()> {
    let (connection, screen) = x11rb::connect(display)?;
    let root = connection.setup().roots[screen].root;
    let preview = DragPreview::new(&connection, screen, label, icon, scale)?;
    while !cancel.load(Ordering::Relaxed) {
        let pointer = connection.query_pointer(root)?.reply()?;
        if !pointer.mask.contains(KeyButMask::BUTTON1) {
            break;
        }
        preview.move_to(&connection, pointer.root_x, pointer.root_y)?;
        connection.flush()?;
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

struct DragPreview {
    window: Window,
    width: u16,
    height: u16,
    screen_width: u16,
    screen_height: u16,
}

impl DragPreview {
    fn new(
        connection: &RustConnection,
        screen_index: usize,
        label: &str,
        icon: &[u8],
        scale: f32,
    ) -> X11Result<Self> {
        let version = connection.shape_query_version()?.reply()?;
        if (version.major_version, version.minor_version) < (1, 1) {
            return Err("X11 input shapes are unavailable".into());
        }
        let scale = scale.clamp(1.0, 3.0);
        let fonts = eframe::egui::FontDefinitions::default();
        let font_data = fonts
            .font_data
            .get("Ubuntu-Light")
            .ok_or("UI font is unavailable")?;
        let font = FontRef::try_from_slice(font_data.bytes())?;
        let font_size = 15.0 * scale;
        let scaled_font = font.as_scaled(PxScale::from(font_size));
        let max_text_width = 250.0 * scale;
        let mut text = String::new();
        let mut text_width = 0.0;
        for character in label.chars() {
            let advance = scaled_font.h_advance(scaled_font.glyph_id(character));
            if text_width + advance > max_text_width - font_size {
                text.push('…');
                text_width += scaled_font.h_advance(scaled_font.glyph_id('…'));
                break;
            }
            text.push(character);
            text_width += advance;
        }
        let icon_size = (32.0 * scale).round() as u32;
        let padding = (8.0 * scale).round() as u32;
        let width = (padding * 3 + icon_size + text_width.ceil() as u32).max(64) as u16;
        let height = (padding * 2 + icon_size) as u16;
        let mut canvas = image::RgbaImage::from_pixel(
            u32::from(width),
            u32::from(height),
            image::Rgba([250, 250, 250, 255]),
        );
        for x in 0..u32::from(width) {
            canvas.put_pixel(x, 0, image::Rgba([120, 120, 120, 255]));
            canvas.put_pixel(x, u32::from(height) - 1, image::Rgba([120, 120, 120, 255]));
        }
        for y in 0..u32::from(height) {
            canvas.put_pixel(0, y, image::Rgba([120, 120, 120, 255]));
            canvas.put_pixel(u32::from(width) - 1, y, image::Rgba([120, 120, 120, 255]));
        }
        let icon = image::load_from_memory(icon)?
            .resize_exact(icon_size, icon_size, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        image::imageops::overlay(&mut canvas, &icon, i64::from(padding), i64::from(padding));
        let mut x = (padding * 2 + icon_size) as f32;
        let baseline = (f32::from(height) - scaled_font.height()) / 2.0 + scaled_font.ascent();
        for character in text.chars() {
            let glyph = scaled_font
                .glyph_id(character)
                .with_scale_and_position(font_size, point(x, baseline));
            x += scaled_font.h_advance(glyph.id);
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|glyph_x, glyph_y, coverage| {
                    let pixel_x = bounds.min.x as i32 + glyph_x as i32;
                    let pixel_y = bounds.min.y as i32 + glyph_y as i32;
                    if pixel_x >= 0
                        && pixel_y >= 0
                        && pixel_x < i32::from(width)
                        && pixel_y < i32::from(height)
                    {
                        let pixel = canvas.get_pixel_mut(pixel_x as u32, pixel_y as u32);
                        for channel in &mut pixel.0[..3] {
                            *channel =
                                (f32::from(*channel) * (1.0 - coverage) + 32.0 * coverage) as u8;
                        }
                    }
                });
            }
        }
        let screen = &connection.setup().roots[screen_index];
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| &depth.visuals)
            .find(|visual| visual.visual_id == screen.root_visual)
            .ok_or("X11 root visual is unavailable")?;
        if visual.red_mask == 0 || visual.green_mask == 0 || visual.blue_mask == 0 {
            return Err("X11 root visual does not support RGB colors".into());
        }
        let mut image = x11rb::image::Image::allocate_native(
            width,
            height,
            screen.root_depth,
            connection.setup(),
        )?;
        for (x, y, pixel) in canvas.enumerate_pixels() {
            let channel = |value: u8, mask: u32| {
                let shift = mask.trailing_zeros();
                ((u32::from(value) * (mask >> shift) / 255) << shift) & mask
            };
            let value = channel(pixel[0], visual.red_mask)
                | channel(pixel[1], visual.green_mask)
                | channel(pixel[2], visual.blue_mask);
            image.put_pixel(x as u16, y as u16, value);
        }
        let pixmap = connection.generate_id()?;
        let gc = connection.generate_id()?;
        let window = connection.generate_id()?;
        connection.create_pixmap(screen.root_depth, pixmap, screen.root, width, height)?;
        connection.create_gc(gc, pixmap, &CreateGCAux::new())?;
        for request in image.put(connection, pixmap, gc, 0, 0)? {
            request.check()?;
        }
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                screen.root,
                0,
                0,
                width,
                height,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    .background_pixmap(pixmap)
                    .override_redirect(1),
            )?
            .check()?;
        connection
            .shape_rectangles(
                shape::SO::SET,
                shape::SK::INPUT,
                ClipOrdering::UNSORTED,
                window,
                0,
                0,
                &[],
            )?
            .check()?;
        connection.free_gc(gc)?;
        connection.free_pixmap(pixmap)?;
        Ok(Self {
            window,
            width,
            height,
            screen_width: screen.width_in_pixels,
            screen_height: screen.height_in_pixels,
        })
    }

    fn move_to(&self, connection: &RustConnection, x: i16, y: i16) -> X11Result<()> {
        let x =
            (i32::from(x) + 16).clamp(0, i32::from(self.screen_width.saturating_sub(self.width)));
        let y =
            (i32::from(y) + 20).clamp(0, i32::from(self.screen_height.saturating_sub(self.height)));
        connection.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .stack_mode(StackMode::ABOVE),
        )?;
        connection.map_window(self.window)?;
        Ok(())
    }
}

fn send_message(
    connection: &RustConnection,
    target: Window,
    kind: Atom,
    data: [u32; 5],
) -> X11Result<()> {
    let event = ClientMessageEvent::new(32, target, kind, data);
    connection.send_event(false, target, EventMask::NO_EVENT, event)?;
    Ok(())
}

fn target_at_pointer(
    connection: &RustConnection,
    root: Window,
    aware: Atom,
    origin: Window,
) -> X11Result<Option<(Window, u32)>> {
    let mut window = root;
    let mut target = None;
    loop {
        let child = connection.query_pointer(window)?.reply()?.child;
        if child == NONE {
            break;
        }
        window = child;
        if window == origin {
            return Ok(None);
        }
        let property = connection
            .get_property(false, window, aware, AtomEnum::ATOM, 0, 1)?
            .reply()?;
        if let Some(version) = property.value32().and_then(|mut values| values.next())
            && version >= 3
        {
            target = Some((window, version.min(5)));
        }
    }
    Ok(target)
}

fn run_drag(data: Vec<u8>, display: Option<&str>, origin: Window) -> X11Result<()> {
    let (connection, screen) = x11rb::connect(display)?;
    let root = connection.setup().roots[screen].root;
    let atoms = Atoms::new(&connection)?.reply()?;
    let source = connection.generate_id()?;
    connection.create_window(
        COPY_DEPTH_FROM_PARENT,
        source,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )?;
    connection.set_selection_owner(source, atoms.XdndSelection, CURRENT_TIME)?;
    connection.flush()?;
    if connection
        .get_selection_owner(atoms.XdndSelection)?
        .reply()?
        .owner
        != source
    {
        return Err("Could not own the X11 drag selection".into());
    }
    let mut target = None;
    let mut accepted = false;
    let mut status_received = false;
    let mut released_at = None;
    let mut dropped_at = None;
    loop {
        while let Some(event) = connection.poll_for_event()? {
            match event {
                Event::ClientMessage(event) if event.type_ == atoms.XdndStatus => {
                    let values = event.data.as_data32();
                    if target.is_some_and(|(window, _)| values[0] == window) {
                        accepted = values[1] & 1 != 0;
                        status_received = true;
                    }
                }
                Event::ClientMessage(event) if event.type_ == atoms.XdndFinished => return Ok(()),
                Event::SelectionRequest(event) if event.selection == atoms.XdndSelection => {
                    let property = if event.property == NONE {
                        event.target
                    } else {
                        event.property
                    };
                    let supported = if event.target == atoms.text_uri_list {
                        connection.change_property8(
                            PropMode::REPLACE,
                            event.requestor,
                            property,
                            atoms.text_uri_list,
                            &data,
                        )?;
                        true
                    } else if event.target == atoms.TARGETS {
                        connection.change_property32(
                            PropMode::REPLACE,
                            event.requestor,
                            property,
                            AtomEnum::ATOM,
                            &[atoms.TARGETS, atoms.text_uri_list],
                        )?;
                        true
                    } else {
                        false
                    };
                    let reply = SelectionNotifyEvent {
                        response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: event.time,
                        requestor: event.requestor,
                        selection: event.selection,
                        target: event.target,
                        property: if supported { property } else { NONE },
                    };
                    connection.send_event(false, event.requestor, EventMask::NO_EVENT, reply)?;
                }
                Event::SelectionClear(event) if event.selection == atoms.XdndSelection => {
                    return Err("X11 drag selection was lost".into());
                }
                _ => {}
            }
        }
        if let Some(started) = dropped_at {
            if Instant::now().duration_since(started) > Duration::from_secs(30) {
                return Err("X11 drop target did not finish the drag".into());
            }
        } else {
            let pointer = connection.query_pointer(root)?.reply()?;
            let next_target = target_at_pointer(&connection, root, atoms.XdndAware, origin)?;
            if next_target != target {
                if let Some((window, _)) = target {
                    send_message(&connection, window, atoms.XdndLeave, [source, 0, 0, 0, 0])?;
                }
                target = next_target;
                accepted = false;
                status_received = false;
                if let Some((window, version)) = target {
                    send_message(
                        &connection,
                        window,
                        atoms.XdndEnter,
                        [source, version << 24, atoms.text_uri_list, 0, 0],
                    )?;
                }
            }
            if let Some((window, _)) = target {
                send_message(
                    &connection,
                    window,
                    atoms.XdndPosition,
                    [
                        source,
                        0,
                        pack_position(pointer.root_x, pointer.root_y),
                        CURRENT_TIME,
                        atoms.XdndActionCopy,
                    ],
                )?;
            }
            if !pointer.mask.contains(KeyButMask::BUTTON1) {
                let released = *released_at.get_or_insert_with(Instant::now);
                if status_received
                    || target.is_none()
                    || released.elapsed() > Duration::from_secs(1)
                {
                    if let Some((window, _)) = target.filter(|_| accepted) {
                        send_message(
                            &connection,
                            window,
                            atoms.XdndDrop,
                            [source, 0, CURRENT_TIME, 0, 0],
                        )?;
                        dropped_at = Some(Instant::now());
                    } else {
                        if let Some((window, _)) = target {
                            send_message(
                                &connection,
                                window,
                                atoms.XdndLeave,
                                [source, 0, 0, 0, 0],
                            )?;
                        }
                        connection.flush()?;
                        return Ok(());
                    }
                }
            }
        }
        connection.flush()?;
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn pack_position(x: i16, y: i16) -> u32 {
    (u32::from(x as u16) << 16) | u32::from(y as u16)
}

fn preview_label(paths: &[PathBuf]) -> String {
    let name = paths
        .first()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if paths.len() > 1 {
        format!("{} items: {name}", paths.len())
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_preserves_signed_screen_coordinates() {
        assert_eq!(pack_position(-10, 25), 0xfff6_0019);
    }

    #[test]
    fn preview_label_includes_name_and_extra_item_count() {
        assert_eq!(preview_label(&[PathBuf::from("/tmp/a.txt")]), "a.txt");
        assert_eq!(
            preview_label(&[PathBuf::from("/tmp/Grüße"), PathBuf::from("/tmp/b")]),
            "2 items: Grüße"
        );
    }

    #[test]
    #[ignore = "requires an isolated X11 display in ZARG_X11_TEST_DISPLAY"]
    fn x11_drag_transfers_uri_list_to_drop_target() {
        transfer_uri_list(false);
    }

    #[test]
    #[ignore = "requires an isolated X11 display in ZARG_X11_TEST_DISPLAY"]
    fn x11_drag_waits_for_status_after_quick_release() {
        transfer_uri_list(true);
    }

    #[test]
    #[ignore = "requires an isolated X11 display and ZARG_X11_GUI_BIN and ZARG_X11_CLI_BIN"]
    fn gui_drag_transfers_extracted_file_to_x11_target() {
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        let display = std::env::var("ZARG_X11_TEST_DISPLAY").unwrap();
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("payload.txt");
        let archive = temp.path().join("example.zar");
        std::fs::write(&input, b"X11 GUI drag test").unwrap();
        assert!(
            std::process::Command::new(std::env::var_os("ZARG_X11_CLI_BIN").unwrap())
                .args(["create", "--no-progress"])
                .arg(&archive)
                .arg(&input)
                .status()
                .unwrap()
                .success()
        );
        let (connection, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen].root;
        let dimensions = &connection.setup().roots[screen];
        assert!(dimensions.width_in_pixels >= 1500 && dimensions.height_in_pixels >= 900);
        let atoms = Atoms::new(&connection).unwrap().reply().unwrap();
        let target = connection.generate_id().unwrap();
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                target,
                root,
                1300,
                100,
                200,
                200,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        connection
            .change_property32(
                PropMode::REPLACE,
                target,
                atoms.XdndAware,
                AtomEnum::ATOM,
                &[5],
            )
            .unwrap();
        connection.map_window(target).unwrap();
        connection.flush().unwrap();

        let mut gui = Child(
            std::process::Command::new(std::env::var_os("ZARG_X11_GUI_BIN").unwrap())
                .arg(&archive)
                .env("DISPLAY", &display)
                .env_remove("WAYLAND_DISPLAY")
                .env("HOME", temp.path())
                .env("XDG_CONFIG_HOME", temp.path())
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while connection
            .query_tree(root)
            .unwrap()
            .reply()
            .unwrap()
            .children
            .len()
            < 2
        {
            assert!(
                gui.0.try_wait().unwrap().is_none(),
                "GUI exited during startup"
            );
            assert!(Instant::now() < deadline, "GUI did not create its window");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_secs(2));
        for (x, y, button) in [
            (65, 130, 0),
            (65, 130, x11rb::protocol::xproto::BUTTON_PRESS_EVENT),
            (90, 145, 0),
            (600, 145, 0),
            (1190, 145, 0),
            (1350, 145, 0),
        ] {
            if button == 0 {
                connection
                    .warp_pointer(NONE, root, 0, 0, 0, 0, x, y)
                    .unwrap();
            } else {
                x11rb::protocol::xtest::fake_input(
                    &connection,
                    button,
                    1,
                    CURRENT_TIME,
                    root,
                    0,
                    0,
                    0,
                )
                .unwrap();
            }
            connection.flush().unwrap();
            std::thread::sleep(Duration::from_millis(150));
        }
        let mut source = NONE;
        let mut received = false;
        while Instant::now() < deadline {
            if let Some(event) = connection.poll_for_event().unwrap() {
                match event {
                    Event::ClientMessage(event) if event.type_ == atoms.XdndEnter => {
                        source = event.data.as_data32()[0];
                    }
                    Event::ClientMessage(event) if event.type_ == atoms.XdndPosition => {
                        send_message(
                            &connection,
                            source,
                            atoms.XdndStatus,
                            [target, 1, 0, 0, atoms.XdndActionCopy],
                        )
                        .unwrap();
                        x11rb::protocol::xtest::fake_input(
                            &connection,
                            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT,
                            1,
                            CURRENT_TIME,
                            root,
                            0,
                            0,
                            0,
                        )
                        .unwrap();
                        connection.flush().unwrap();
                    }
                    Event::ClientMessage(event) if event.type_ == atoms.XdndDrop => {
                        connection
                            .convert_selection(
                                target,
                                atoms.XdndSelection,
                                atoms.text_uri_list,
                                atoms.text_uri_list,
                                CURRENT_TIME,
                            )
                            .unwrap();
                        connection.flush().unwrap();
                    }
                    Event::SelectionNotify(event) => {
                        assert_eq!(event.property, atoms.text_uri_list);
                        let property = connection
                            .get_property(
                                false,
                                target,
                                atoms.text_uri_list,
                                atoms.text_uri_list,
                                0,
                                1024,
                            )
                            .unwrap()
                            .reply()
                            .unwrap();
                        let uri = std::str::from_utf8(&property.value).unwrap();
                        let path = uri.strip_prefix("file://").unwrap().trim_end();
                        assert_eq!(std::fs::read(path).unwrap(), b"X11 GUI drag test");
                        send_message(
                            &connection,
                            source,
                            atoms.XdndFinished,
                            [target, 1, atoms.XdndActionCopy, 0, 0],
                        )
                        .unwrap();
                        connection.flush().unwrap();
                        received = true;
                        break;
                    }
                    _ => {}
                }
            } else {
                assert!(
                    gui.0.try_wait().unwrap().is_none(),
                    "GUI exited during drag"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(received, "GUI did not transfer a file to the X11 target");
    }

    #[test]
    #[ignore = "requires an isolated X11 display in ZARG_X11_TEST_DISPLAY"]
    fn returning_to_origin_cancels_the_external_drop() {
        let display = std::env::var("ZARG_X11_TEST_DISPLAY").unwrap();
        let (connection, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen].root;
        let atoms = Atoms::new(&connection).unwrap().reply().unwrap();
        let frame = connection.generate_id().unwrap();
        let origin = connection.generate_id().unwrap();
        let child = connection.generate_id().unwrap();
        let external = connection.generate_id().unwrap();
        for (window, parent, x, y, width, height) in [
            (frame, root, 0, 0, 200, 200),
            (origin, frame, 10, 10, 180, 180),
            (child, origin, 10, 10, 100, 100),
            (external, root, 400, 0, 200, 200),
        ] {
            connection
                .create_window(
                    COPY_DEPTH_FROM_PARENT,
                    window,
                    parent,
                    x,
                    y,
                    width,
                    height,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new(),
                )
                .unwrap();
            connection
                .change_property32(
                    PropMode::REPLACE,
                    window,
                    atoms.XdndAware,
                    AtomEnum::ATOM,
                    &[5],
                )
                .unwrap();
            connection.map_window(window).unwrap();
        }
        connection
            .warp_pointer(NONE, external, 0, 0, 0, 0, 50, 50)
            .unwrap();
        x11rb::protocol::xtest::fake_input(
            &connection,
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT,
            1,
            CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap();
        connection.flush().unwrap();
        let worker = std::thread::spawn(move || {
            run_drag(b"file:///tmp/test\r\n".to_vec(), Some(&display), origin)
                .map_err(|error| error.to_string())
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut source = NONE;
        let mut returned = false;
        let mut left = false;
        while Instant::now() < deadline {
            if let Some(Event::ClientMessage(event)) = connection.poll_for_event().unwrap() {
                if event.type_ == atoms.XdndEnter {
                    source = event.data.as_data32()[0];
                } else if event.type_ == atoms.XdndPosition && !returned {
                    send_message(
                        &connection,
                        source,
                        atoms.XdndStatus,
                        [external, 1, 0, 0, atoms.XdndActionCopy],
                    )
                    .unwrap();
                    connection
                        .warp_pointer(NONE, child, 0, 0, 0, 0, 30, 30)
                        .unwrap();
                    connection.flush().unwrap();
                    returned = true;
                } else if event.type_ == atoms.XdndLeave {
                    left = true;
                    break;
                } else if event.type_ == atoms.XdndDrop {
                    panic!("a drop was sent after returning to the origin");
                }
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(
            returned && left,
            "the external target did not receive a leave event"
        );
        x11rb::protocol::xtest::fake_input(
            &connection,
            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT,
            1,
            CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap();
        connection.flush().unwrap();
        assert_eq!(
            target_at_pointer(&connection, root, atoms.XdndAware, origin).unwrap(),
            None
        );
        assert_eq!(
            target_at_pointer(&connection, root, atoms.XdndAware, NONE).unwrap(),
            Some((child, 5))
        );
        worker.join().unwrap().unwrap();
    }

    #[test]
    #[ignore = "requires an isolated X11 display in ZARG_X11_TEST_DISPLAY"]
    fn preview_is_visible_moves_and_does_not_intercept_input() {
        use x11rb::protocol::xproto::MapState;

        let display = std::env::var("ZARG_X11_TEST_DISPLAY").unwrap();
        let (connection, screen_index) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen_index].root;
        let target = connection.generate_id().unwrap();
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                target,
                root,
                0,
                0,
                300,
                200,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        connection.map_window(target).unwrap();
        connection
            .warp_pointer(NONE, target, 0, 0, 0, 0, 60, 60)
            .unwrap();
        let preview = DragPreview::new(
            &connection,
            screen_index,
            "Grüße.txt (+1)",
            include_bytes!("../assets/flat/mimetypes/text-plain.png"),
            1.0,
        )
        .unwrap();
        preview.move_to(&connection, 34, 30).unwrap();
        connection.flush().unwrap();
        assert_eq!(
            connection
                .get_window_attributes(preview.window)
                .unwrap()
                .reply()
                .unwrap()
                .map_state,
            MapState::VIEWABLE
        );
        assert_eq!(
            connection
                .query_pointer(root)
                .unwrap()
                .reply()
                .unwrap()
                .child,
            target
        );
        assert!(
            connection
                .shape_get_rectangles(preview.window, shape::SK::INPUT)
                .unwrap()
                .reply()
                .unwrap()
                .rectangles
                .is_empty()
        );
        let (pixels, _) = x11rb::image::Image::get(
            &connection,
            preview.window,
            0,
            0,
            preview.width,
            preview.height,
        )
        .unwrap();
        let background = pixels.get_pixel(4, 4);
        assert!((8..40).any(|x| (8..40).any(|y| pixels.get_pixel(x, y) != background)));
        assert!(
            (48..preview.width - 8)
                .any(|x| (8..preview.height - 8).any(|y| pixels.get_pixel(x, y) != background))
        );
        preview.move_to(&connection, 90, 80).unwrap();
        let geometry = connection
            .get_geometry(preview.window)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!((geometry.x, geometry.y), (106, 100));
    }

    #[test]
    #[ignore = "requires an isolated X11 display in ZARG_X11_TEST_DISPLAY"]
    fn preview_starts_before_external_drag_and_ends_on_release() {
        use x11rb::protocol::xproto::MapState;

        let display = std::env::var("ZARG_X11_TEST_DISPLAY").unwrap();
        let (connection, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen].root;
        let atoms = Atoms::new(&connection).unwrap().reply().unwrap();
        connection
            .warp_pointer(NONE, root, 0, 0, 0, 0, 50, 50)
            .unwrap();
        x11rb::protocol::xtest::fake_input(
            &connection,
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT,
            1,
            CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap();
        connection.flush().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = std::thread::spawn(move || {
            run_preview(
                Some(&display),
                "example.txt",
                include_bytes!("../assets/flat/mimetypes/text-plain.png"),
                1.0,
                &worker_cancel,
            )
            .map_err(|error| error.to_string())
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut window = None;
        while Instant::now() < deadline {
            for child in connection
                .query_tree(root)
                .unwrap()
                .reply()
                .unwrap()
                .children
            {
                let attributes = connection
                    .get_window_attributes(child)
                    .unwrap()
                    .reply()
                    .unwrap();
                if attributes.override_redirect && attributes.map_state == MapState::VIEWABLE {
                    window = Some(child);
                    break;
                }
            }
            if window.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let window = window.expect("preview did not appear during the drag gesture");
        assert_eq!(
            connection
                .get_selection_owner(atoms.XdndSelection)
                .unwrap()
                .reply()
                .unwrap()
                .owner,
            NONE
        );
        x11rb::protocol::xtest::fake_input(
            &connection,
            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT,
            1,
            CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap();
        connection.flush().unwrap();
        worker.join().unwrap().unwrap();
        assert!(
            !connection
                .query_tree(root)
                .unwrap()
                .reply()
                .unwrap()
                .children
                .contains(&window)
        );
    }

    fn transfer_uri_list(quick_release: bool) {
        let display = std::env::var("ZARG_X11_TEST_DISPLAY").unwrap();
        let (connection, screen) = x11rb::connect(Some(&display)).unwrap();
        let root = connection.setup().roots[screen].root;
        let atoms = Atoms::new(&connection).unwrap().reply().unwrap();
        let target = connection.generate_id().unwrap();
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                target,
                root,
                0,
                0,
                200,
                200,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        connection
            .change_property32(
                PropMode::REPLACE,
                target,
                atoms.XdndAware,
                AtomEnum::ATOM,
                &[5],
            )
            .unwrap();
        connection.map_window(target).unwrap();
        connection
            .warp_pointer(NONE, target, 0, 0, 0, 0, 50, 50)
            .unwrap();
        x11rb::protocol::xtest::fake_input(
            &connection,
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT,
            1,
            CURRENT_TIME,
            root,
            0,
            0,
            0,
        )
        .unwrap();
        connection.flush().unwrap();
        let expected = b"file:///tmp/test%20file\r\n".to_vec();
        let mut drag = X11Drag::new(NONE);
        drag.start_on_display(
            &[PathBuf::from("/tmp/test file")],
            &eframe::egui::Context::default(),
            Some(display),
        )
        .unwrap();
        assert!(drag.active());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut positions = 0;
        let mut source = NONE;
        let mut received = false;
        while Instant::now() < deadline {
            if let Some(event) = connection.poll_for_event().unwrap() {
                match event {
                    Event::ClientMessage(event) if event.type_ == atoms.XdndEnter => {
                        source = event.data.as_data32()[0];
                    }
                    Event::ClientMessage(event) if event.type_ == atoms.XdndPosition => {
                        positions += 1;
                        send_message(
                            &connection,
                            source,
                            atoms.XdndStatus,
                            [target, 1, 0, 0, atoms.XdndActionCopy],
                        )
                        .unwrap();
                        if positions == if quick_release { 1 } else { 2 } {
                            x11rb::protocol::xtest::fake_input(
                                &connection,
                                x11rb::protocol::xproto::BUTTON_RELEASE_EVENT,
                                1,
                                CURRENT_TIME,
                                root,
                                0,
                                0,
                                0,
                            )
                            .unwrap();
                        }
                        connection.flush().unwrap();
                    }
                    Event::ClientMessage(event) if event.type_ == atoms.XdndDrop => {
                        connection
                            .convert_selection(
                                target,
                                atoms.XdndSelection,
                                atoms.text_uri_list,
                                atoms.text_uri_list,
                                CURRENT_TIME,
                            )
                            .unwrap();
                        connection.flush().unwrap();
                    }
                    Event::SelectionNotify(event) => {
                        assert_eq!(event.property, atoms.text_uri_list);
                        let property = connection
                            .get_property(
                                false,
                                target,
                                atoms.text_uri_list,
                                atoms.text_uri_list,
                                0,
                                1024,
                            )
                            .unwrap()
                            .reply()
                            .unwrap();
                        assert_eq!(property.value, expected);
                        send_message(
                            &connection,
                            source,
                            atoms.XdndFinished,
                            [target, 1, atoms.XdndActionCopy, 0, 0],
                        )
                        .unwrap();
                        connection.flush().unwrap();
                        received = true;
                        break;
                    }
                    _ => {}
                }
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert!(received, "XDND transfer did not complete");
        while drag.active() && Instant::now() < deadline {
            drag.poll().unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!drag.active(), "X11 drag worker did not finish");
    }
}
