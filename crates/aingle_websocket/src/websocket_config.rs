//! defines a builder-style config struct for setting up websockets

/// A builder-style config struct for setting up websockets.
#[derive(Debug)]
pub struct WebsocketConfig {
    /// Scheme to use for urls - e.g. "ws" or "wss". [default = "ws"]
    pub scheme: &'static str,

    /// Seconds after which the lib will stop tracking individual request ids.
    /// [default = 30]
    pub default_request_timeout_s: usize,

    /// We will treat the socket as disconnected if we receive no messages
    /// in this timeframe, using the tcp keepalive mechanism.
    /// [default = 10]
    pub tcp_keepalive_s: usize,

    /// How many messages can be buffered in the internal channel. [default = 10]
    pub channel_buffer_size: usize,

    /// Initial size of the write buffer in bytes. [default = 128 KiB]
    pub write_buffer_size: usize,

    /// Maximum size of the write buffer in bytes. [default = 4 MiB]
    pub max_write_buffer_size: usize,

    /// Maximum total message size of a websocket message. [default = 64M]
    pub max_message_size: usize,

    /// Maximum websocket frame size. [default = 16M]
    pub max_frame_size: usize,

    /// Maximum number of pending new incoming connections. [default = 255]
    pub max_pending_connections: usize,
}

impl Default for WebsocketConfig {
    fn default() -> Self {
        Self {
            scheme: "ws",
            default_request_timeout_s: 30,
            tcp_keepalive_s: 30,
            channel_buffer_size: 10,
            write_buffer_size: 128 << 10,   // 128 KiB
            max_write_buffer_size: 4 << 20, // 4 MiB
            max_message_size: 64 << 20,
            max_frame_size: 16 << 20,
            max_pending_connections: 255,
        }
    }
}

impl WebsocketConfig {
    /// Builder-style setter.
    pub fn scheme(mut self, scheme: &'static str) -> Self {
        self.scheme = scheme;
        self
    }

    /// Builder-style setter.
    pub fn default_request_timeout_s(mut self, s: usize) -> Self {
        self.default_request_timeout_s = s;
        self
    }

    /// Builder-style setter.
    pub fn tcp_keepalive_s(mut self, s: usize) -> Self {
        self.tcp_keepalive_s = s;
        self
    }

    /// Builder-style setter.
    pub fn channel_buffer_size(mut self, size: usize) -> Self {
        self.channel_buffer_size = size;
        self
    }

    /// Builder-style setter.
    pub fn write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }

    /// Builder-style setter.
    pub fn max_write_buffer_size(mut self, max: usize) -> Self {
        self.max_write_buffer_size = max;
        self
    }

    /// Builder-style setter.
    pub fn max_message_size(mut self, max: usize) -> Self {
        self.max_message_size = max;
        self
    }

    /// Builder-style setter.
    pub fn max_frame_size(mut self, max: usize) -> Self {
        self.max_frame_size = max;
        self
    }
}

/// internal helper to convert our configs into tungstenite configs
pub(crate) trait TungsteniteConfigExt {
    /// generate a low-level tungstenite config from our high-level config
    fn to_tungstenite(&self) -> tungstenite::protocol::WebSocketConfig;
}

impl TungsteniteConfigExt for WebsocketConfig {
    fn to_tungstenite(&self) -> tungstenite::protocol::WebSocketConfig {
        tungstenite::protocol::WebSocketConfig {
            write_buffer_size: self.write_buffer_size,
            max_write_buffer_size: self.max_write_buffer_size,
            max_message_size: Some(self.max_message_size),
            max_frame_size: Some(self.max_frame_size),
            ..Default::default()
        }
    }
}
