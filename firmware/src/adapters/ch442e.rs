#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Mcu,
    Sink,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pins {
    pub in_high: bool,
    pub en_n_high: bool,
}

impl Pins {
    pub const fn boot_safe() -> Self {
        Self {
            in_high: false,
            en_n_high: true,
        }
    }

    pub const fn default_mcu() -> Self {
        Self {
            in_high: false,
            en_n_high: false,
        }
    }

    pub const fn sink_route() -> Self {
        Self {
            in_high: true,
            en_n_high: false,
        }
    }

    pub const fn route(self) -> Route {
        if self.en_n_high {
            Route::Disabled
        } else if self.in_high {
            Route::Sink
        } else {
            Route::Mcu
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_is_disabled_when_enable_n_is_high() {
        let pins = Pins {
            in_high: false,
            en_n_high: true,
        };
        assert_eq!(pins.route(), Route::Disabled);
    }

    #[test]
    fn route_maps_to_mcu_and_sink_when_enabled() {
        assert_eq!(Pins::default_mcu().route(), Route::Mcu);
        assert_eq!(Pins::sink_route().route(), Route::Sink);
    }

    #[test]
    fn boot_then_init_results_in_default_mcu_route() {
        assert_eq!(Pins::boot_safe().route(), Route::Disabled);
        assert_eq!(Pins::default_mcu().route(), Route::Mcu);
    }
}
