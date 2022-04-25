use alloc::sync::Arc;
use core::time;

use embedded_svc::{
    errors::Errors,
    ws::{FrameType, Sender},
};
use esp_idf_hal::{
    delay::TickType,
    mutex::{Condvar, Mutex},
};
use esp_idf_sys::{
    c_types, esp, esp_event_base_t, esp_websocket_client_close, esp_websocket_client_config_t,
    esp_websocket_client_destroy, esp_websocket_client_handle_t, esp_websocket_client_init,
    esp_websocket_client_is_connected, esp_websocket_client_send_bin,
    esp_websocket_client_send_text, esp_websocket_client_start, esp_websocket_event_data_t,
    esp_websocket_event_id_t_WEBSOCKET_EVENT_ANY, esp_websocket_event_id_t_WEBSOCKET_EVENT_CLOSED,
    esp_websocket_event_id_t_WEBSOCKET_EVENT_CONNECTED,
    esp_websocket_event_id_t_WEBSOCKET_EVENT_DATA,
    esp_websocket_event_id_t_WEBSOCKET_EVENT_DISCONNECTED,
    esp_websocket_event_id_t_WEBSOCKET_EVENT_ERROR, esp_websocket_event_id_t_WEBSOCKET_EVENT_MAX,
    esp_websocket_register_events, esp_websocket_transport_t,
    esp_websocket_transport_t_WEBSOCKET_TRANSPORT_OVER_SSL,
    esp_websocket_transport_t_WEBSOCKET_TRANSPORT_OVER_TCP,
    esp_websocket_transport_t_WEBSOCKET_TRANSPORT_UNKNOWN, EspError, TickType_t, ESP_FAIL,
};

use crate::private::common::Newtype;
use crate::private::cstr::{self, RawCstrs};

pub enum EspWebSocketTransport {
    TransportUnknown,
    TransportOverTCP,
    TransportOverSSL,
}

impl Default for EspWebSocketTransport {
    fn default() -> Self {
        Self::TransportUnknown
    }
}

impl From<EspWebSocketTransport> for Newtype<esp_websocket_transport_t> {
    fn from(transport: EspWebSocketTransport) -> Self {
        Newtype(match transport {
            EspWebSocketTransport::TransportUnknown => {
                esp_websocket_transport_t_WEBSOCKET_TRANSPORT_UNKNOWN
            }
            EspWebSocketTransport::TransportOverTCP => {
                esp_websocket_transport_t_WEBSOCKET_TRANSPORT_OVER_TCP
            }
            EspWebSocketTransport::TransportOverSSL => {
                esp_websocket_transport_t_WEBSOCKET_TRANSPORT_OVER_SSL
            }
        })
    }
}

pub enum WebSocketEvent<'a> {
    Connected,
    Disconnected,
    Closed,
    Text(alloc::borrow::Cow<'a, str>),
    Binary(&'a [u8]),
}

impl<'a> WebSocketEvent<'a> {
    // TODO: error
    pub fn from_raw_event(
        event_id: i32,
        event_data: *mut esp_websocket_event_data_t,
    ) -> Result<Self, ()> {
        #[allow(non_upper_case_globals)]
        match event_id {
            esp_websocket_event_id_t_WEBSOCKET_EVENT_ERROR => {
                // TODO: error
                Err(())
            }
            esp_websocket_event_id_t_WEBSOCKET_EVENT_CONNECTED => Ok(Self::Connected),
            esp_websocket_event_id_t_WEBSOCKET_EVENT_DISCONNECTED => Ok(Self::Disconnected),
            esp_websocket_event_id_t_WEBSOCKET_EVENT_DATA => {
                let event_data = unsafe { *event_data };
                match event_data.op_code {
                    // Text frame
                    1 => Ok(Self::Text(cstr::from_cstr_ptr(event_data.data_ptr))),
                    // Binary frame
                    2 => Ok(Self::Binary(unsafe {
                        std::slice::from_raw_parts(
                            event_data.data_ptr as *const u8,
                            event_data.data_len as usize,
                        )
                    })),
                    // TODO: error
                    _ => Err(()),
                }
            }
            esp_websocket_event_id_t_WEBSOCKET_EVENT_CLOSED => Ok(Self::Closed),
            esp_websocket_event_id_t_WEBSOCKET_EVENT_MAX => {
                // TODO: error
                Err(())
            }
            _ => {
                // TODO: error
                Err(())
            }
        }
    }
}

#[derive(Default)]
pub struct EspWebSocketClientConfig<'a> {
    pub uri: Option<&'a str>,
    pub host: Option<&'a str>,
    pub port: u16,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub path: Option<&'a str>,
    pub disable_auto_reconnect: bool,
    // TODO: pub user_context:
    pub task_prio: u8,
    pub task_stack: u8,
    pub buffer_size: usize,
    pub transport: EspWebSocketTransport,
    pub subprotocol: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub headers: Option<&'a str>,
    pub pingpong_timeout_sec: time::Duration,
    pub disable_pingpong_discon: bool,
    pub use_global_ca_store: bool,
    pub skip_cert_common_name_check: bool,
    pub keep_alive_idle: Option<time::Duration>,
    pub keep_alive_interval: Option<time::Duration>,
    pub keep_alive_count: Option<u16>,
    pub reconnect_timeout_ms: time::Duration,
    pub network_timeout_ms: time::Duration,
    pub ping_interval_sec: time::Duration,
    // TODO: pub if_name:

    // TODO: implement
    // pub cert_pem: Option<&'a str>,
    // pub client_cert: Option<&'a str>,
    // pub client_key: Option<&'a str>,
}

impl<'a> From<EspWebSocketClientConfig<'a>> for (esp_websocket_client_config_t, RawCstrs) {
    fn from(conf: EspWebSocketClientConfig) -> Self {
        let mut cstrs = RawCstrs::new();

        let mut c_conf = esp_websocket_client_config_t {
            uri: cstrs.as_nptr(conf.uri),
            host: cstrs.as_nptr(conf.host),
            port: conf.port.into(),
            username: cstrs.as_nptr(conf.username),
            password: cstrs.as_nptr(conf.password),
            path: cstrs.as_nptr(conf.path),
            disable_auto_reconnect: conf.disable_auto_reconnect,
            // TODO user_context: *mut c_types::c_void,
            user_context: core::ptr::null_mut(),

            task_prio: conf.task_prio as _,
            task_stack: conf.task_stack as _,
            buffer_size: conf.buffer_size as _,

            transport: Newtype::<esp_websocket_transport_t>::from(conf.transport).0,

            subprotocol: cstrs.as_nptr(conf.subprotocol),
            user_agent: cstrs.as_nptr(conf.user_agent),
            headers: cstrs.as_nptr(conf.headers),

            pingpong_timeout_sec: conf.pingpong_timeout_sec.as_secs() as _,
            disable_pingpong_discon: conf.disable_pingpong_discon,

            use_global_ca_store: conf.use_global_ca_store,
            skip_cert_common_name_check: conf.skip_cert_common_name_check,

            // default keep_alive_* values are overwritten later in this function
            ping_interval_sec: conf.ping_interval_sec.as_secs() as _,

            // TODO if_name: *mut ifreq,
            if_name: core::ptr::null_mut(),

            ..Default::default()
            // For the following, not yet implemented fields
            // pub cert_pem: *const c_types::c_char,
            // pub cert_len: size_t,
            // pub client_cert: *const c_types::c_char,
            // pub client_cert_len: size_t,
            // pub client_key: *const c_types::c_char,
            // pub client_key_len: size_t,
        };

        if let Some(idle) = conf.keep_alive_idle {
            c_conf.keep_alive_enable = true;
            c_conf.keep_alive_idle = idle.as_secs() as _;
        }

        if let Some(interval) = conf.keep_alive_interval {
            c_conf.keep_alive_enable = true;
            c_conf.keep_alive_interval = interval.as_secs() as _;
        }

        if let Some(count) = conf.keep_alive_count {
            c_conf.keep_alive_enable = true;
            c_conf.keep_alive_count = count.into();
        }

        if let Some(keep_alive_idle) = conf.keep_alive_idle {
            c_conf.keep_alive_enable = true;
            c_conf.keep_alive_idle = keep_alive_idle.as_secs() as _;
        }

        (c_conf, cstrs)
    }
}

struct UnsafeCallback(*mut Box<dyn FnMut(i32, *mut esp_websocket_event_data_t)>);

impl UnsafeCallback {
    fn from(boxed: &mut Box<Box<dyn FnMut(i32, *mut esp_websocket_event_data_t)>>) -> Self {
        Self(boxed.as_mut())
    }

    unsafe fn from_ptr(ptr: *mut c_types::c_void) -> Self {
        Self(ptr as *mut _)
    }

    fn as_ptr(&self) -> *mut c_types::c_void {
        self.0 as *mut _
    }

    unsafe fn call(&self, data: *mut esp_websocket_event_data_t, event_id: i32) {
        let reference = self.0.as_mut().unwrap();

        // TODO: more elegant, somewhere else?
        if event_id == esp_websocket_event_id_t_WEBSOCKET_EVENT_DATA {
            (reference)(event_id, data);
        }
    }
}

#[cfg_attr(version("1.61"), allow(suspicious_auto_trait_impls))]
unsafe impl Send for Newtype<*mut esp_websocket_event_data_t> {}

struct EspWebSocketConnectionState<'a> {
    message: Mutex<Option<WebSocketEvent<'a>>>,
    posted: Condvar,
    processed: Condvar,
}

impl<'a> Default for EspWebSocketConnectionState<'a> {
    fn default() -> Self {
        Self {
            message: Mutex::new(None),
            posted: Condvar::new(),
            processed: Condvar::new(),
        }
    }
}

#[derive(Clone)]
pub struct EspWebSocketConnection<'a>(Arc<EspWebSocketConnectionState<'a>>);

impl<'a> Default for EspWebSocketConnection<'a> {
    fn default() -> Self {
        Self(Arc::new(EspWebSocketConnectionState::default()))
    }
}

impl<'a> EspWebSocketConnection<'a> {
    fn post(&self, event: WebSocketEvent<'a>) {
        let mut message = self.0.message.lock();

        while message.is_some() {
            message = self.0.processed.wait(message);
        }

        *message = Some(event);
        self.0.posted.notify_all();

        while message.is_some() {
            message = self.0.processed.wait(message);
        }
    }

    // TODO iterator
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<WebSocketEvent<'a>> {
        let mut message = self.0.message.lock();

        // wait for new message to arrive
        while message.is_none() {
            message = self.0.posted.wait(message);
        }

        if message.is_none() {
            *message = None;
            self.0.processed.notify_all();

            return None;
        }

        *message
    }
}

// TODO: handle non-data frames
pub struct EspWebSocketEventData<'a> {
    pub frame_type: FrameType,
    pub data: Option<&'a [i8]>,
    state: &'a Arc<EspWebSocketConnectionState<'a>>,
}

impl<'a> EspWebSocketEventData<'a> {
    fn new_from_raw(
        event: &'a esp_websocket_event_data_t,
        state: &'a Arc<EspWebSocketConnectionState<'a>>,
    ) -> Option<Self> {
        let frame_type = Self::frame_type_from_op_code(event.op_code);

        if event.data_ptr.is_null() {
            return None;
        }

        if event.data_len > 0 {
            return Some(Self {
                frame_type,
                data: Some(unsafe {
                    std::slice::from_raw_parts(event.data_ptr, event.data_len as _)
                }),
                state,
            });
        }

        Some(Self {
            frame_type,
            data: None,
            state,
        })
    }

    // TODO: move somewhere more fitting
    fn frame_type_from_op_code(op_code: u8) -> FrameType {
        // https://datatracker.ietf.org/doc/html/rfc6455#page-66
        match op_code {
            0 => FrameType::Continue(true), // TODO: check if more frames are coming
            1 => FrameType::Text(false),    // TODO: assuming that all the data has been sent
            2 => FrameType::Binary(false),  // TODO: like Text
            8 => FrameType::Close,
            9 => FrameType::Ping,
            10 => FrameType::Pong,
            other => panic!("Unknown frame type: {}", other),
        }
    }
}

impl<'a> Drop for EspWebSocketEventData<'a> {
    fn drop(&mut self) {
        let mut message = self.state.message.lock();

        if message.is_some() {
            *message = None;
            self.state.processed.notify_all();
        }
    }
}

// TODO: replace EspWebSocketClientConfig with a builder interface
pub struct EspWebSocketClient {
    handle: esp_websocket_client_handle_t,
    // used for the timeout in every call to a send method in the c lib as the
    // `send` method in the `Sender` trait in embedded_svc::ws does not take a timeout itself
    timeout: TickType_t,
    callback: Box<dyn FnMut(i32, *mut esp_websocket_event_data_t)>,
}

impl EspWebSocketClient {
    pub fn new(
        config: EspWebSocketClientConfig,
        timeout: time::Duration,
    ) -> Result<(Self, EspWebSocketConnection), EspError> {
        let connection = EspWebSocketConnection::default();
        let client_connection = connection.clone();

        let client = Self::new_with_raw_callback(
            config,
            timeout,
            Box::new(move |event_id, event_handle| {
                match WebSocketEvent::from_raw_event(event_id, event_handle) {
                    Ok(v) => EspWebSocketConnection::post(&client_connection, v),
                    // TODO: error
                    Err(e) => todo!(),
                };
            }),
        )?;

        Ok((client, connection))
    }

    fn new_with_raw_callback(
        config: EspWebSocketClientConfig,
        timeout: time::Duration,
        raw_callback: Box<dyn FnMut(i32, *mut esp_websocket_event_data_t)>,
    ) -> Result<Self, EspError> {
        let mut boxed_raw_callback = Box::new(raw_callback);
        let unsafe_callback = UnsafeCallback::from(&mut boxed_raw_callback);

        let t: TickType = timeout.into();

        let (conf, _cstrs): (esp_websocket_client_config_t, RawCstrs) = config.into();
        let handle = unsafe { esp_websocket_client_init(&conf) };

        if handle.is_null() {
            esp!(ESP_FAIL)?;
        }

        let client = Self {
            handle,
            timeout: t.0,
            callback: boxed_raw_callback,
        };

        esp!(unsafe {
            esp_websocket_register_events(
                client.handle,
                esp_websocket_event_id_t_WEBSOCKET_EVENT_ANY,
                Some(Self::handle),
                unsafe_callback.as_ptr(),
            )
        })?;

        esp!(unsafe { esp_websocket_client_start(handle) })?;

        Ok(client)
    }

    extern "C" fn handle(
        event_handler_arg: *mut c_types::c_void,
        _event_base: esp_event_base_t,
        event_id: i32,
        event_data: *mut c_types::c_void,
    ) {
        unsafe {
            UnsafeCallback::from_ptr(event_handler_arg).call(event_data as _, event_id);
        }
    }

    fn check(result: c_types::c_int) -> Result<usize, EspError> {
        if result < 0 {
            esp!(result)?;
        }

        Ok(result as _)
    }

    fn send_data(
        &mut self,
        frame_type: FrameType,
        frame_data: Option<&[u8]>,
    ) -> Result<usize, <EspWebSocketClient as Errors>::Error> {
        let mut content = core::ptr::null();
        let mut content_length: usize = 0;

        if let Some(data) = frame_data {
            content = data.as_ref().as_ptr();
            content_length = data.as_ref().len();
        }

        Self::check(match frame_type {
            FrameType::Binary(false) => unsafe {
                esp_websocket_client_send_bin(
                    self.handle,
                    content as _,
                    content_length as _,
                    self.timeout,
                )
            },
            FrameType::Text(false) => unsafe {
                esp_websocket_client_send_text(
                    self.handle,
                    content as _,
                    content_length as _,
                    self.timeout,
                )
            },
            _ => {
                unimplemented!();
            }
        })
    }
}

impl Drop for EspWebSocketClient {
    fn drop(&mut self) {
        esp!(unsafe { esp_websocket_client_close(self.handle, self.timeout) }).unwrap();
        esp!(unsafe { esp_websocket_client_destroy(self.handle) }).unwrap();

        // timeout and callback dropped automatically
    }
}

impl Errors for EspWebSocketClient {
    type Error = EspError;
}

impl Sender for EspWebSocketClient {
    fn send(
        &mut self,
        frame_type: FrameType,
        frame_data: Option<&[u8]>,
    ) -> Result<(), Self::Error> {
        while !unsafe { esp_websocket_client_is_connected(self.handle) } {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        match frame_type {
            FrameType::Binary(false) | FrameType::Text(false) => {
                self.send_data(frame_type, frame_data)?
            }
            FrameType::Binary(true) | FrameType::Text(true) => todo!(),
            FrameType::Ping | FrameType::Pong => {
                unimplemented!("Handled automatically by the wrapped C library")
            }
            FrameType::Close => todo!(),
            FrameType::SocketClose => todo!(),
            FrameType::Continue(_) => todo!(),
        };

        Ok(())
    }
}
