use crate::{content_aead, encoding_options, parse_encryption_scope, password_set, runtime_error};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use zarlib::operations::{self, StreamEncryption, StreamInput};

type CallbackError = Arc<Mutex<Option<PyErr>>>;

struct PythonStream {
    object: Py<PyAny>,
    error: CallbackError,
}

impl PythonStream {
    fn call<T>(
        &self,
        f: impl FnOnce(Python<'_>, &Bound<'_, PyAny>) -> PyResult<T>,
    ) -> io::Result<T> {
        Python::attach(|py| match f(py, self.object.bind(py)) {
            Ok(value) => Ok(value),
            Err(error) => {
                let mut first = self.error.lock().expect("callback error lock poisoned");
                if first.is_none() {
                    *first = Some(error);
                }
                Err(io::Error::other("Python stream callback failed"))
            }
        })
    }
}

impl Read for PythonStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.call(|_, object| {
            let result = object.call_method1("read", (buf.len(),))?;
            let bytes = result
                .cast::<PyBytes>()
                .map_err(|_| PyTypeError::new_err("stream read() must return bytes"))?
                .as_bytes();
            if bytes.len() > buf.len() {
                return Err(PyValueError::new_err(
                    "stream read() returned too many bytes",
                ));
            }
            buf[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        })
    }
}

impl Write for PythonStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.call(|py, object| {
            let result = object.call_method1("write", (PyBytes::new(py, buf),))?;
            if result.is_instance_of::<PyBool>() {
                return Err(PyTypeError::new_err(
                    "stream write() must return an integer byte count",
                ));
            }
            let count = result.extract::<usize>().map_err(|_| {
                PyTypeError::new_err("stream write() must return a nonnegative integer byte count")
            })?;
            if count == 0 || count > buf.len() {
                return Err(PyValueError::new_err(
                    "stream write() returned an invalid byte count",
                ));
            }
            Ok(count)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for PythonStream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.call(|_, object| {
            let result = match pos {
                SeekFrom::Start(offset) => object.call_method1("seek", (offset, 0))?,
                SeekFrom::Current(offset) => object.call_method1("seek", (offset, 1))?,
                SeekFrom::End(offset) => object.call_method1("seek", (offset, 2))?,
            };
            if result.is_instance_of::<PyBool>() {
                return Err(PyTypeError::new_err(
                    "stream seek() must return an integer position",
                ));
            }
            result.extract::<u64>().map_err(|_| {
                PyTypeError::new_err("stream seek() must return a nonnegative integer position")
            })
        })
    }
}

fn finish<T>(result: Result<T, String>, error: &CallbackError) -> PyResult<T> {
    if let Some(error) = error.lock().expect("callback error lock poisoned").take() {
        return Err(error);
    }
    result.map_err(runtime_error)
}

#[pyfunction]
#[pyo3(signature = (archive, inputs, *, block_size = 8388608, compression_level = 3, password = None, aead = "aes-256-gcm", encryption_scope = "header-and-data"))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_archive_from_streams(
    py: Python<'_>,
    archive: Py<PyAny>,
    inputs: Vec<(String, Py<PyAny>)>,
    block_size: usize,
    compression_level: i32,
    password: Option<Vec<u8>>,
    aead: &str,
    encryption_scope: &str,
) -> PyResult<()> {
    let options = encoding_options(block_size, compression_level)?;
    if block_size == 0 {
        return Err(PyValueError::new_err(
            "streaming block_size must be between 64 KiB and 32 MiB",
        ));
    }
    let algorithm = content_aead(aead)?;
    let scope = parse_encryption_scope(encryption_scope)?;
    let error = Arc::new(Mutex::new(None));
    let mut archive = PythonStream {
        object: archive,
        error: error.clone(),
    };
    let mut streams: Vec<_> = inputs
        .into_iter()
        .map(|(path, object)| {
            (
                path,
                PythonStream {
                    object,
                    error: error.clone(),
                },
            )
        })
        .collect();
    let result = py.detach(|| {
        let mut inputs: Vec<_> = streams
            .iter_mut()
            .map(|(path, reader)| StreamInput { path, reader })
            .collect();
        let encryption = password.as_deref().map(|password| StreamEncryption {
            password,
            algorithm,
            scope,
        });
        operations::create_archive_from_readers(&mut archive, &mut inputs, options, encryption)
            .map_err(|error| error.to_string())
    });
    finish(result, &error)
}

#[pyfunction]
#[pyo3(signature = (archive, path, output, *, to = None, password = None, passwords = None))]
pub(crate) fn extract_file_to_stream(
    py: Python<'_>,
    archive: Py<PyAny>,
    path: String,
    output: Py<PyAny>,
    to: Option<String>,
    password: Option<Vec<u8>>,
    passwords: Option<HashMap<String, Vec<u8>>>,
) -> PyResult<u64> {
    let passwords = password_set(password, passwords)?;
    let error = Arc::new(Mutex::new(None));
    let mut archive = PythonStream {
        object: archive,
        error: error.clone(),
    };
    let mut output = PythonStream {
        object: output,
        error: error.clone(),
    };
    let result = py.detach(|| {
        operations::extract_single_file_to_writer(
            &mut archive,
            &path,
            &mut output,
            &passwords,
            to.as_deref(),
        )
        .map_err(|error| error.to_string())
    });
    finish(result, &error)
}
