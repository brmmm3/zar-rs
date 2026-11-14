use eframe::egui;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Duration;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_pointer, wl_registry,
    wl_seat, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};

struct Seat {
    seat: wl_seat::WlSeat,
    pointer: Option<wl_pointer::WlPointer>,
    device: Option<wl_data_device::WlDataDevice>,
    drag_offer: Option<wl_data_offer::WlDataOffer>,
    selection_offer: Option<wl_data_offer::WlDataOffer>,
    pointer_state: PointerState,
}

#[derive(Default)]
struct PointerState {
    focused: bool,
    pressed: Option<(u32, u64)>,
}

impl PointerState {
    fn focus(&mut self, focused: bool) {
        self.focused = focused;
        self.pressed = None;
    }

    fn press(&mut self, serial: u32, order: u64) {
        if self.focused {
            self.pressed = Some((serial, order));
        }
    }

    fn release(&mut self) {
        self.pressed = None;
    }

    fn matches(&self, press: DragPress) -> bool {
        self.focused && self.pressed == Some((press.serial, press.order))
    }
}

struct State {
    surface: wl_surface::WlSurface,
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seats: BTreeMap<u32, Seat>,
    active: Option<wl_data_source::WlDataSource>,
    sequence: u64,
    errors: mpsc::Sender<String>,
    context: egui::Context,
    offers: HashMap<ObjectId, OfferInfo>,
    transfers: HashMap<u64, wl_data_offer::WlDataOffer>,
    transfer_sender: mpsc::Sender<(u64, Result<Vec<PathBuf>, String>)>,
    next_transfer: u64,
}

pub struct WaylandDrag {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    errors: mpsc::Receiver<String>,
    transfer_receiver: mpsc::Receiver<(u64, Result<Vec<PathBuf>, String>)>,
    dropped_paths: Vec<Vec<PathBuf>>,
}

#[derive(Default)]
struct OfferInfo {
    uri_list: bool,
    accepted: bool,
    action: Option<wl_data_device_manager::DndAction>,
}

const URI_LIST_MIME: &str = "text/uri-list";
const MAX_URI_LIST_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DragPress {
    seat: u32,
    serial: u32,
    order: u64,
}

impl WaylandDrag {
    pub fn new(frame: &eframe::Frame, context: &egui::Context) -> Result<Option<Self>, String> {
        let display = frame.display_handle().map_err(|error| error.to_string())?;
        let window = frame.window_handle().map_err(|error| error.to_string())?;
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) =
            (display.as_raw(), window.as_raw())
        else {
            return Ok(None);
        };

        // The app drops this guest connection in on_exit, before eframe destroys the window.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        // The surface belongs to the same live display and remains owned by eframe.
        let id = unsafe {
            ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                window.surface.as_ptr().cast(),
            )
        }
        .map_err(|error| error.to_string())?;
        let surface =
            wl_surface::WlSurface::from_id(&connection, id).map_err(|error| error.to_string())?;
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let (sender, errors) = mpsc::channel();
        let (transfer_sender, transfer_receiver) = mpsc::channel();
        let mut state = State {
            surface,
            manager: None,
            seats: BTreeMap::new(),
            active: None,
            sequence: 0,
            errors: sender,
            context: context.clone(),
            offers: HashMap::new(),
            transfers: HashMap::new(),
            transfer_sender,
            next_transfer: 0,
        };
        queue
            .roundtrip(&mut state)
            .map_err(|error| error.to_string())?;
        queue
            .roundtrip(&mut state)
            .map_err(|error| error.to_string())?;
        if state.manager.is_none() {
            return Err("Wayland compositor does not provide data-device manager version 3".into());
        }
        Ok(Some(Self {
            connection,
            queue,
            state,
            errors,
            transfer_receiver,
            dropped_paths: Vec::new(),
        }))
    }

    pub fn poll(&mut self) -> Result<(), String> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| error.to_string())?;
        while let Ok((id, result)) = self.transfer_receiver.try_recv() {
            if let Some(offer) = self.state.transfers.remove(&id) {
                match result {
                    Ok(paths) => {
                        offer.finish();
                        self.dropped_paths.push(paths);
                    }
                    Err(error) => {
                        let _ = self.state.errors.send(error);
                    }
                }
                self.state.offers.remove(&offer.id());
                offer.destroy();
            }
        }
        self.connection.flush().map_err(|error| error.to_string())?;
        if let Ok(error) = self.errors.try_recv() {
            return Err(error);
        }
        Ok(())
    }

    pub fn take_dropped_paths(&mut self) -> Vec<Vec<PathBuf>> {
        std::mem::take(&mut self.dropped_paths)
    }

    pub fn active(&self) -> bool {
        self.state.active.is_some()
    }

    pub fn pressed(&self) -> Option<DragPress> {
        self.state
            .seats
            .iter()
            .filter_map(|(name, seat)| {
                let (serial, order) = seat.pointer_state.pressed?;
                seat.device.as_ref()?;
                Some(DragPress {
                    seat: *name,
                    serial,
                    order,
                })
            })
            .max_by_key(|press| press.order)
    }

    pub fn press_is_active(&self, press: DragPress) -> bool {
        self.state
            .seats
            .get(&press.seat)
            .is_some_and(|seat| seat.pointer_state.matches(press))
    }

    pub fn start(&mut self, paths: &[PathBuf], press: DragPress) -> Result<(), String> {
        self.poll()?;
        if self.state.active.is_some() {
            return Err("A Wayland drag is already in progress".into());
        }
        let seat = self
            .state
            .seats
            .get(&press.seat)
            .filter(|seat| seat.pointer_state.matches(press))
            .and_then(|seat| seat.device.as_ref())
            .ok_or("The original mouse press ended before the selected items were ready")?;
        let data = uri_list(paths)?;
        let manager = self
            .state
            .manager
            .as_ref()
            .ok_or("Wayland data-device manager is unavailable")?;
        let source = manager.create_data_source(&self.queue.handle(), Arc::new(data));
        source.offer("text/uri-list".to_owned());
        if source.version() >= 3 {
            source.set_actions(wl_data_device_manager::DndAction::Copy);
        }
        seat.start_drag(Some(&source), &self.state.surface, None, press.serial);
        self.state.active = Some(source);
        self.connection.flush().map_err(|error| error.to_string())
    }
}

impl Drop for WaylandDrag {
    fn drop(&mut self) {
        if let Some(source) = self.state.active.take() {
            source.destroy();
        }
        for (_, offer) in self.state.transfers.drain() {
            offer.destroy();
        }
        for (_, seat) in std::mem::take(&mut self.state.seats) {
            if let Some(offer) = seat.drag_offer {
                offer.destroy();
            }
            if let Some(offer) = seat.selection_offer {
                offer.destroy();
            }
            if let Some(pointer) = seat.pointer
                && pointer.version() >= 3
            {
                pointer.release();
            }
            if let Some(device) = seat.device
                && device.version() >= 2
            {
                device.release();
            }
            if seat.seat.version() >= 5 {
                seat.seat.release();
            }
        }
        if let Err(error) = self.connection.flush() {
            eprintln!("zarg: could not clean up Wayland drag resources: {error}");
        }
    }
}

fn uri_list(paths: &[PathBuf]) -> Result<Vec<u8>, String> {
    let mut result = String::new();
    for path in paths {
        let url = url::Url::from_file_path(path)
            .map_err(|_| format!("Cannot convert {} to a file URI", path.display()))?;
        result.push_str(url.as_str());
        result.push_str("\r\n");
    }
    if result.is_empty() {
        return Err("No files to drag".into());
    }
    Ok(result.into_bytes())
}

fn parse_uri_list(data: &[u8]) -> Result<Vec<PathBuf>, String> {
    let text = std::str::from_utf8(data).map_err(|error| format!("Invalid URI list: {error}"))?;
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bytes = line.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (index + 2 >= bytes.len()
                    || !bytes[index + 1].is_ascii_hexdigit()
                    || !bytes[index + 2].is_ascii_hexdigit())
            {
                return Err(format!("Invalid escape in dropped URI: {line}"));
            }
        }
        let url = url::Url::parse(line).map_err(|error| format!("Invalid dropped URI: {error}"))?;
        if url.scheme() != "file" || url.query().is_some() || url.fragment().is_some() {
            return Err(format!("Dropped URI is not a local file: {line}"));
        }
        let path = url
            .to_file_path()
            .map_err(|_| format!("Dropped URI is not a local file: {line}"))?;
        if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(format!("Invalid dropped file path: {line}"));
        }
        paths.push(path);
    }
    if paths.is_empty() {
        return Err("The dropped URI list contains no files".into());
    }
    Ok(paths)
}

impl State {
    fn receive_drop(&mut self, offer: &wl_data_offer::WlDataOffer) -> Result<(), String> {
        let (reader, writer) =
            UnixStream::pair().map_err(|error| format!("Cannot receive dropped files: {error}"))?;
        reader
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| format!("Cannot set drop transfer timeout: {error}"))?;
        offer.receive(URI_LIST_MIME.to_owned(), writer.as_fd());
        drop(writer);

        self.next_transfer += 1;
        let id = self.next_transfer;
        let sender = self.transfer_sender.clone();
        let context = self.context.clone();
        std::thread::Builder::new()
            .name("zarg-wayland-drop".into())
            .spawn(move || {
                let mut data = Vec::new();
                let result = reader
                    .take(MAX_URI_LIST_BYTES + 1)
                    .read_to_end(&mut data)
                    .map_err(|error| format!("Cannot read dropped files: {error}"))
                    .and_then(|_| {
                        if data.len() as u64 > MAX_URI_LIST_BYTES {
                            return Err("Dropped URI list is too large".into());
                        }
                        parse_uri_list(&data)
                    });
                let _ = sender.send((id, result));
                context.request_repaint();
            })
            .map_err(|error| format!("Cannot start drop transfer: {error}"))?;
        self.transfers.insert(id, offer.clone());
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_data_device_manager"
                && version >= 3
                && state.manager.is_none() =>
            {
                let manager = registry.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
                    name,
                    version.min(3),
                    handle,
                    (),
                );
                for (name, seat) in &mut state.seats {
                    seat.device = Some(manager.get_data_device(&seat.seat, handle, *name));
                }
                state.manager = Some(manager);
            }
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_seat" => {
                let seat =
                    registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(7), handle, name);
                let device = state
                    .manager
                    .as_ref()
                    .map(|manager| manager.get_data_device(&seat, handle, name));
                state.seats.insert(
                    name,
                    Seat {
                        seat,
                        pointer: None,
                        device,
                        drag_offer: None,
                        selection_offer: None,
                        pointer_state: PointerState::default(),
                    },
                );
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(seat) = state.seats.remove(&name) {
                    if let Some(offer) = seat.drag_offer {
                        state.offers.remove(&offer.id());
                        offer.destroy();
                    }
                    if let Some(offer) = seat.selection_offer {
                        state.offers.remove(&offer.id());
                        offer.destroy();
                    }
                    if let Some(pointer) = seat.pointer
                        && pointer.version() >= 3
                    {
                        pointer.release();
                    }
                    if let Some(device) = seat.device
                        && device.version() >= 2
                    {
                        device.release();
                    }
                    if seat.seat.version() >= 5 {
                        seat.seat.release();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, u32> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        name: &u32,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && let Some(data) = state.seats.get_mut(name)
        {
            if capabilities.contains(wl_seat::Capability::Pointer) {
                if data.pointer.is_none() {
                    data.pointer = Some(seat.get_pointer(handle, *name));
                }
            } else if let Some(pointer) = data.pointer.take() {
                data.pointer_state.focus(false);
                if pointer.version() >= 3 {
                    pointer.release();
                }
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.seats.get_mut(name) else {
            return;
        };
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                seat.pointer_state
                    .focus(surface.id().protocol_id() == state.surface.id().protocol_id());
            }
            wl_pointer::Event::Leave { .. } => {
                seat.pointer_state.focus(false);
            }
            wl_pointer::Event::Button {
                serial,
                button: 0x110,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } if seat.pointer_state.focused => {
                state.sequence += 1;
                seat.pointer_state.press(serial, state.sequence);
            }
            wl_pointer::Event::Button {
                button: 0x110,
                state: WEnum::Value(wl_pointer::ButtonState::Released),
                ..
            } => seat.pointer_state.release(),
            _ => {}
        }
    }
}

fn send_uri_list(
    fd: OwnedFd,
    data: Arc<Vec<u8>>,
    errors: mpsc::Sender<String>,
    context: egui::Context,
) {
    std::thread::spawn(move || {
        if let Err(error) = std::fs::File::from(fd).write_all(&data) {
            let _ = errors.send(format!("Cannot send dragged files: {error}"));
            context.request_repaint();
        }
    });
}

impl Dispatch<wl_data_source::WlDataSource, Arc<Vec<u8>>> for State {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        data: &Arc<Vec<u8>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                if mime_type != "text/uri-list" {
                    let _ = state
                        .errors
                        .send(format!("Unsupported Wayland drag format: {mime_type}"));
                    return;
                }
                send_uri_list(
                    fd,
                    Arc::clone(data),
                    state.errors.clone(),
                    state.context.clone(),
                );
            }
            wl_data_source::Event::Cancelled | wl_data_source::Event::DndFinished => {
                source.destroy();
                if state.active.as_ref() == Some(source) {
                    state.active = None;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_device::Event::DataOffer { id } = event {
            state.offers.insert(id.id(), OfferInfo::default());
            return;
        }
        let Some(seat) = state.seats.get_mut(name) else {
            return;
        };
        match event {
            wl_data_device::Event::Enter {
                id,
                serial,
                surface,
                ..
            } => {
                if let Some(offer) = &id {
                    let acceptable = state.active.is_none()
                        && surface.id().protocol_id() == state.surface.id().protocol_id()
                        && state
                            .offers
                            .get(&offer.id())
                            .is_some_and(|info| info.uri_list);
                    if let Some(info) = state.offers.get_mut(&offer.id()) {
                        info.accepted = acceptable;
                    }
                    offer.accept(serial, acceptable.then(|| URI_LIST_MIME.to_owned()));
                    if acceptable {
                        offer.set_actions(
                            wl_data_device_manager::DndAction::Copy,
                            wl_data_device_manager::DndAction::Copy,
                        );
                    }
                }
                if let Some(old) = std::mem::replace(&mut seat.drag_offer, id) {
                    state.offers.remove(&old.id());
                    old.destroy();
                }
            }
            wl_data_device::Event::Leave => {
                if let Some(offer) = seat.drag_offer.take() {
                    state.offers.remove(&offer.id());
                    offer.destroy();
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(offer) = seat.drag_offer.take() {
                    let accepted = state.offers.get(&offer.id()).is_some_and(|info| {
                        info.accepted
                            && info.action == Some(wl_data_device_manager::DndAction::Copy)
                    });
                    let result = if accepted {
                        state.receive_drop(&offer)
                    } else {
                        Err("The dropped files could not be accepted for copying".into())
                    };
                    if let Err(error) = result {
                        let _ = state.errors.send(error);
                        state.offers.remove(&offer.id());
                        offer.destroy();
                    }
                }
            }
            wl_data_device::Event::Selection { id } => {
                if let Some(old) = std::mem::replace(&mut seat.selection_offer, id) {
                    state.offers.remove(&old.id());
                    old.destroy();
                }
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(info) = state.offers.get_mut(&offer.id()) {
            match event {
                wl_data_offer::Event::Offer { mime_type } if mime_type == URI_LIST_MIME => {
                    info.uri_list = true;
                }
                wl_data_offer::Event::Action { dnd_action } => {
                    info.action = match dnd_action {
                        WEnum::Value(action) => Some(action),
                        WEnum::Unknown(_) => None,
                    };
                }
                _ => {}
            }
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn outgoing_uri_requests_are_answered_before_drop() {
        let (mut reader, writer) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (errors, error_receiver) = mpsc::channel();
        send_uri_list(
            writer.into(),
            Arc::new(b"file:///tmp/ready\r\n".to_vec()),
            errors,
            egui::Context::default(),
        );
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"file:///tmp/ready\r\n");
        assert!(error_receiver.try_recv().is_err());
    }

    #[test]
    fn uri_list_encodes_paths_and_separates_entries() {
        let paths = [
            PathBuf::from("/tmp/a b#%.txt"),
            PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff\n".to_vec())),
        ];
        assert_eq!(
            uri_list(&paths).unwrap(),
            b"file:///tmp/a%20b%23%25.txt\r\nfile:///tmp/%FF%0A\r\n"
        );
    }

    #[test]
    fn uri_list_rejects_empty_and_relative_paths() {
        assert!(uri_list(&[]).is_err());
        assert!(uri_list(&[PathBuf::from("relative")]).is_err());
    }

    #[test]
    fn incoming_uri_list_decodes_files_folders_and_comments() {
        let paths = parse_uri_list(
            b"# file manager\r\nfile:///tmp/a%20b%23.txt\r\nfile://localhost/tmp/folder/\nfile:///tmp/%FF\n",
        )
        .unwrap();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/a b#.txt"),
                PathBuf::from("/tmp/folder/"),
                PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec())),
            ]
        );
    }

    #[test]
    fn incoming_uri_list_rejects_remote_or_invalid_entries() {
        for data in [
            &b"file://other-host/tmp/file\r\n"[..],
            &b"https://example.org/file\r\n"[..],
            &b"file:///tmp/%zz\r\n"[..],
            &b"file:///tmp/file#fragment\r\n"[..],
            &b"file:///tmp/%00\r\n"[..],
            &b"# no files\r\n"[..],
            &b"file:///tmp/file\r\nhttps://example.org/file\r\n"[..],
        ] {
            assert!(parse_uri_list(data).is_err(), "{data:?}");
        }
    }

    #[test]
    fn only_original_focused_press_can_start_drag() {
        let mut pointer = PointerState::default();
        let press = DragPress {
            seat: 1,
            serial: 42,
            order: 1,
        };
        pointer.press(press.serial, press.order);
        assert!(!pointer.matches(press));
        pointer.focus(true);
        pointer.press(press.serial, press.order);
        assert!(pointer.matches(press));
        pointer.release();
        assert!(!pointer.matches(press));
        pointer.press(press.serial, 2);
        assert!(!pointer.matches(press));
        pointer.focus(false);
        assert!(!pointer.matches(press));
    }
}
