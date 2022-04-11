use core::time;

use embedded_svc::{
    errors::Errors,
    ws::{FrameType, Sender},
};
use esp_idf_hal::delay::TickType;
use esp_idf_sys::*;

use crate::private::common::Newtype;
use crate::private::cstr::RawCstrs;

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

impl<'a> Default for EspWebSocketClientConfig<'a> {
    fn default() -> Self {
        Self {
            uri: None,
            host: None,
            // default port is set by library
            port: 0,
            username: None,
            password: None,
            path: None,
            disable_auto_reconnect: false,
            // TODO: pub user_context:
            task_prio: 0,
            task_stack: 0,
            buffer_size: 0,
            transport: EspWebSocketTransport::default(),
            subprotocol: None,
            user_agent: None,
            headers: None,
            pingpong_timeout_sec: time::Duration::default(),
            disable_pingpong_discon: false,
            use_global_ca_store: false,
            skip_cert_common_name_check: false,
            keep_alive_idle: None,
            keep_alive_interval: None,
            keep_alive_count: None,
            reconnect_timeout_ms: time::Duration::default(),
            network_timeout_ms: time::Duration::default(),
            ping_interval_sec: time::Duration::default(),
            // TODO: pub if_name:
        }
    }
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

            // default keep_alive_* values, might overwritten later in this function
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

pub struct EspWebSocketClient {
    handle: esp_websocket_client_handle_t,
    // used for the timeout in every call to a send method in the c lib as the
    // `send` method in the `Sender` trait in embedded_svc::ws does not take a timeout itself
    timeout: TickType_t,
}

impl EspWebSocketClient {
    pub fn new(
        config: EspWebSocketClientConfig,
        timeout: time::Duration,
    ) -> Result<Self, EspError> {
        let (conf, _cstrs): (esp_websocket_client_config_t, RawCstrs) = config.into();
        let handle = unsafe { esp_websocket_client_init(&conf) };

        if handle.is_null() {
            esp!(ESP_FAIL)?;
        }
        esp!(unsafe { esp_websocket_client_start(handle) })?;

        let t: TickType = timeout.into();

        Ok(Self {
            handle,
            timeout: t.0,
        })
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
        match frame_type {
            FrameType::Binary(false) | FrameType::Text(false) => {
                self.send_data(frame_type, frame_data)?
            }
            FrameType::Binary(true) | FrameType::Text(true) => todo!(),
            FrameType::Ping => todo!(),
            FrameType::Pong => todo!(),
            FrameType::Close => todo!(),
            FrameType::SocketClose => todo!(),
            FrameType::Continue(_) => todo!(),
        };

        Ok(())
    }
}
