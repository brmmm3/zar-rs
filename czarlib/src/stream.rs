use crate::{
    content_aead, encoding_options, encryption_scope, optional_string, password, passwords,
};
use serde_json::Value;
use std::ffi::{CStr, c_char, c_void};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use zarlib::operations::{self, StreamEncryption, StreamInput};

type ReadCallback = unsafe extern "C" fn(*mut c_void, *mut u8, usize, *mut usize) -> i32;
type WriteCallback = unsafe extern "C" fn(*mut c_void, *const u8, usize, *mut usize) -> i32;
type SeekCallback = unsafe extern "C" fn(*mut c_void, i32, i64, *mut u64) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CStream {
    context: *mut c_void,
    read: Option<ReadCallback>,
    write: Option<WriteCallback>,
    seek: Option<SeekCallback>,
}

#[repr(C)]
pub struct CStreamInput {
    path: *const c_char,
    stream: CStream,
}

struct CallbackStream(CStream);

fn callback_error(operation: &str, status: i32) -> io::Error {
    io::Error::other(format!("{operation} callback failed with status {status}"))
}

impl Read for CallbackStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let callback = self
            .0
            .read
            .ok_or_else(|| io::Error::other("missing read callback"))?;
        let mut count = 0;
        let status = unsafe { callback(self.0.context, buf.as_mut_ptr(), buf.len(), &mut count) };
        if status != 0 {
            return Err(callback_error("read", status));
        }
        if count > buf.len() {
            return Err(io::Error::other(
                "read callback returned an out-of-range count",
            ));
        }
        Ok(count)
    }
}

impl Write for CallbackStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let callback = self
            .0
            .write
            .ok_or_else(|| io::Error::other("missing write callback"))?;
        let mut count = 0;
        let status = unsafe { callback(self.0.context, buf.as_ptr(), buf.len(), &mut count) };
        if status != 0 {
            return Err(callback_error("write", status));
        }
        if count > buf.len() {
            return Err(io::Error::other(
                "write callback returned an out-of-range count",
            ));
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for CallbackStream {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let callback = self
            .0
            .seek
            .ok_or_else(|| io::Error::other("missing seek callback"))?;
        let (origin, offset) = match from {
            SeekFrom::Start(offset) => (
                0,
                i64::try_from(offset)
                    .map_err(|_| io::Error::other("seek offset exceeds INT64_MAX"))?,
            ),
            SeekFrom::Current(offset) => (1, offset),
            SeekFrom::End(offset) => (2, offset),
        };
        let mut position = 0;
        let status = unsafe { callback(self.0.context, origin, offset, &mut position) };
        if status != 0 {
            return Err(callback_error("seek", status));
        }
        if position > i64::MAX as u64 || (origin == 0 && position != offset as u64) {
            return Err(io::Error::other(
                "seek callback returned an invalid position",
            ));
        }
        Ok(position)
    }
}

unsafe fn text<'a>(ptr: *const c_char, name: &str) -> Result<&'a str, String> {
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| format!("{name} is not valid UTF-8"))
}

unsafe fn options(ptr: *const c_char) -> Result<Value, String> {
    let text = unsafe { text(ptr, "options JSON") }?;
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("invalid options JSON: {error}"))?;
    if !value.is_object() {
        return Err("options JSON must be an object".into());
    }
    Ok(value)
}

fn finish(result: std::thread::Result<Result<(), String>>, error: *mut *mut c_char) -> i32 {
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(message)) => {
            // C strings cannot represent embedded NULs in paths or error messages.
            if crate::write_string(error, message.replace('\0', "\\0")).is_ok() {
                1
            } else {
                2
            }
        }
        Err(_) => {
            let _ = crate::write_string(error, "internal Rust panic".into());
            3
        }
    }
}

/// # Safety
/// All non-null pointers must be correctly aligned and valid for their documented
/// reads/writes. Callback contexts and buffers must remain valid during the call.
/// Callbacks must not unwind, and output slots must not alias any input or each other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn czarlib_create_archive_from_readers(
    archive: *const CStream,
    inputs: *const CStreamInput,
    input_count: usize,
    options_json: *const c_char,
    error_message: *mut *mut c_char,
) -> i32 {
    if error_message.is_null() || !error_message.is_aligned() {
        return 2;
    }
    unsafe {
        *error_message = std::ptr::null_mut();
    }
    if archive.is_null()
        || !archive.is_aligned()
        || options_json.is_null()
        || input_count > isize::MAX as usize / std::mem::size_of::<CStreamInput>()
        || (input_count != 0 && (inputs.is_null() || !inputs.is_aligned()))
    {
        return 2;
    }
    let archive = unsafe { *archive };
    if archive.write.is_none() || archive.seek.is_none() {
        return 2;
    }
    let inputs = if input_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(inputs, input_count) }
    };
    if inputs
        .iter()
        .any(|input| input.path.is_null() || input.stream.read.is_none())
    {
        return 2;
    }
    finish(
        catch_unwind(AssertUnwindSafe(|| {
            let request = unsafe { options(options_json) }?;
            let encoding_request = match request.get("encoding_options") {
                Some(value) => {
                    if !value.is_object() {
                        return Err("'encoding_options' must be an object".into());
                    }
                    if request.get("block_size").is_some()
                        || request.get("compression_level").is_some()
                    {
                        return Err("do not mix nested and flat encoding options".into());
                    }
                    value
                }
                None => &request,
            };
            let encoding = encoding_options(encoding_request)?;
            for key in ["aead", "encryption_scope"] {
                if request.get(key).is_some_and(|v| !v.is_string()) {
                    return Err(format!("'{key}' must be a string"));
                }
            }
            let pass = password(&request)?;
            let algorithm = content_aead(&request)?;
            let scope = encryption_scope(&request)?;
            let names = inputs
                .iter()
                .map(|input| unsafe { text(input.path, "input path") })
                .collect::<Result<Vec<_>, _>>()?;
            let mut readers: Vec<_> = inputs
                .iter()
                .map(|input| CallbackStream(input.stream))
                .collect();
            let mut stream_inputs: Vec<_> = names
                .iter()
                .zip(readers.iter_mut())
                .map(|(path, reader)| StreamInput { path, reader })
                .collect();
            let encryption = pass.as_deref().map(|password| StreamEncryption {
                password,
                algorithm,
                scope,
            });
            operations::create_archive_from_readers(
                &mut CallbackStream(archive),
                &mut stream_inputs,
                encoding,
                encryption,
            )
            .map_err(|error| error.to_string())
        })),
        error_message,
    )
}

/// # Safety
/// All pointers and callback contexts must remain valid during the call; output
/// slots must not alias inputs or each other. Callbacks must not unwind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn czarlib_extract_single_file_to_writer(
    archive: *const CStream,
    path: *const c_char,
    output: *const CStream,
    options_json: *const c_char,
    written: *mut u64,
    error_message: *mut *mut c_char,
) -> i32 {
    if error_message.is_null() || !error_message.is_aligned() {
        return 2;
    }
    unsafe {
        *error_message = std::ptr::null_mut();
    }
    if written.is_null() || !written.is_aligned() {
        return 2;
    }
    unsafe {
        *written = 0;
    }
    if archive.is_null()
        || !archive.is_aligned()
        || output.is_null()
        || !output.is_aligned()
        || path.is_null()
        || options_json.is_null()
    {
        return 2;
    }
    let archive = unsafe { *archive };
    let output = unsafe { *output };
    if archive.read.is_none() || archive.seek.is_none() || output.write.is_none() {
        return 2;
    }
    finish(
        catch_unwind(AssertUnwindSafe(|| {
            let request = unsafe { options(options_json) }?;
            let path = unsafe { text(path, "path") }?;
            let passwords = passwords(&request)?;
            let to = optional_string(&request, "to")?;
            let count = operations::extract_single_file_to_writer(
                &mut CallbackStream(archive),
                path,
                &mut CallbackStream(output),
                &passwords,
                to,
            )
            .map_err(|error| error.to_string())?;
            unsafe {
                *written = count;
            }
            Ok(())
        })),
        error_message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::io::Cursor;

    unsafe extern "C" fn read(
        context: *mut c_void,
        buffer: *mut u8,
        capacity: usize,
        count: *mut usize,
    ) -> i32 {
        let reader = unsafe { &mut *context.cast::<Cursor<Vec<u8>>>() };
        let buffer = unsafe { std::slice::from_raw_parts_mut(buffer, capacity.min(37)) };
        match reader.read(buffer) {
            Ok(n) => {
                unsafe { *count = n };
                0
            }
            Err(_) => 1,
        }
    }

    unsafe extern "C" fn write(
        context: *mut c_void,
        buffer: *const u8,
        length: usize,
        count: *mut usize,
    ) -> i32 {
        let writer = unsafe { &mut *context.cast::<Cursor<Vec<u8>>>() };
        let buffer = unsafe { std::slice::from_raw_parts(buffer, length.min(41)) };
        match writer.write(buffer) {
            Ok(n) => {
                unsafe { *count = n };
                0
            }
            Err(_) => 1,
        }
    }

    unsafe extern "C" fn seek(
        context: *mut c_void,
        origin: i32,
        offset: i64,
        position: *mut u64,
    ) -> i32 {
        let stream = unsafe { &mut *context.cast::<Cursor<Vec<u8>>>() };
        let from = match origin {
            0 if offset >= 0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return 1,
        };
        match stream.seek(from) {
            Ok(pos) => {
                unsafe { *position = pos };
                0
            }
            Err(_) => 1,
        }
    }

    unsafe extern "C" fn invalid_read(
        _context: *mut c_void,
        _buffer: *mut u8,
        capacity: usize,
        count: *mut usize,
    ) -> i32 {
        unsafe { *count = capacity + 1 };
        0
    }

    fn stream(cursor: &mut Cursor<Vec<u8>>) -> CStream {
        CStream {
            context: (cursor as *mut Cursor<Vec<u8>>).cast(),
            read: Some(read),
            write: Some(write),
            seek: Some(seek),
        }
    }

    #[test]
    fn callback_streams_round_trip_and_reject_bad_callbacks() {
        let data: Vec<u8> = (0..150_000).map(|i| (i % 251) as u8).collect();
        let mut input = Cursor::new(data.clone());
        let mut archive = Cursor::new(Vec::new());
        let name = CString::new("folder/data").unwrap();
        let options = CString::new(r#"{"block_size":65536}"#).unwrap();
        let input_stream = CStreamInput {
            path: name.as_ptr(),
            stream: stream(&mut input),
        };
        let archive_stream = stream(&mut archive);
        let mut error = std::ptr::null_mut();
        let status = unsafe {
            czarlib_create_archive_from_readers(
                &archive_stream,
                &input_stream,
                1,
                options.as_ptr(),
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert!(error.is_null());

        let mut output = Cursor::new(Vec::new());
        let output_stream = stream(&mut output);
        let mut written = 0;
        let status = unsafe {
            czarlib_extract_single_file_to_writer(
                &archive_stream,
                name.as_ptr(),
                &output_stream,
                options.as_ptr(),
                &mut written,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert!(error.is_null());
        assert_eq!(written, data.len() as u64);
        assert_eq!(output.into_inner(), data);

        let mut archive = Cursor::new(Vec::new());
        let archive_stream = stream(&mut archive);
        let bad_input = CStreamInput {
            path: name.as_ptr(),
            stream: CStream {
                read: Some(invalid_read),
                ..stream(&mut input)
            },
        };
        let status = unsafe {
            czarlib_create_archive_from_readers(
                &archive_stream,
                &bad_input,
                1,
                options.as_ptr(),
                &mut error,
            )
        };
        assert_eq!(status, 1);
        assert!(
            unsafe { CStr::from_ptr(error) }
                .to_str()
                .unwrap()
                .contains("out-of-range count")
        );
        unsafe { crate::czarlib_string_free(error) };

        let automatic = CString::new(r#"{"block_size":0}"#).unwrap();
        let mut archive = Cursor::new(Vec::new());
        let archive_stream = stream(&mut archive);
        let status = unsafe {
            czarlib_create_archive_from_readers(
                &archive_stream,
                std::ptr::null(),
                0,
                automatic.as_ptr(),
                &mut error,
            )
        };
        assert_eq!(status, 1);
        assert!(
            unsafe { CStr::from_ptr(error) }
                .to_str()
                .unwrap()
                .contains("block size")
        );
        unsafe { crate::czarlib_string_free(error) };
    }
}
