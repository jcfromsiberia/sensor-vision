pub mod model;

pub mod client {
    pub mod mqtt {
        pub use mqtt_client::*;
        pub use mqtt_listener::*;
        mod mqtt_client;
        mod mqtt_listener;
    }

    pub mod state {
        pub use scheme::*;
        pub use sensors_state::*;

        mod scheme;
        mod sensors_state;

        #[path = "state_queries.rs"]
        pub mod queries;
    }

    pub mod client;
    pub mod client_queries;
}

pub mod tui_app {
    pub mod dialog {
        pub use confirmation::*;
        pub use generic::*;
        pub use input::*;

        pub mod render;

        mod confirmation;
        mod generic;
        mod input;
    }

    pub mod ui_state {
        pub use state::*;
        mod state;

        pub mod render;

        #[path = "state_queries.rs"]
        pub mod queries;
    }

    pub mod app;
    pub mod tui;
}
