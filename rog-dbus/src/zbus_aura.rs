//! # `DBus` interface proxy for: `org.asuslinux.Daemon`
//!
//! This code was generated by `zbus-xmlgen` `1.0.0` from `DBus` introspection
//! data. Source: `Interface '/org/asuslinux/Aura' from service
//! 'org.asuslinux.Daemon' on system bus`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://zeenix.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This `DBus` object implements
//! [standard `DBus` interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::PeerProxy`]
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use std::collections::BTreeMap;

use rog_aura::keyboard::{LaptopAuraPower, UsbPackets};
use rog_aura::{AuraDeviceType, AuraEffect, AuraModeNum, AuraZone, LedBrightness, PowerZones};
use zbus::blocking::Connection;
use zbus::{proxy, Result};

const BLOCKING_TIME: u64 = 33; // 100ms = 10 FPS, max 50ms = 20 FPS, 40ms = 25 FPS

#[proxy(
    interface = "org.asuslinux.Aura",
    default_service = "org.asuslinux.Daemon",
    default_path = "/org/asuslinux/Aura"
)]
pub trait Aura {
    /// AllModeData method
    fn all_mode_data(&self) -> zbus::Result<BTreeMap<AuraModeNum, AuraEffect>>;

    /// DirectAddressingRaw method
    fn direct_addressing_raw(&self, data: UsbPackets) -> zbus::Result<()>;

    /// Brightness property
    #[zbus(property)]
    fn brightness(&self) -> zbus::Result<LedBrightness>;
    #[zbus(property)]
    fn set_brightness(&self, value: LedBrightness) -> zbus::Result<()>;

    /// DeviceType property
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<AuraDeviceType>;

    /// LedMode property
    #[zbus(property)]
    fn led_mode(&self) -> zbus::Result<AuraModeNum>;
    #[zbus(property)]
    fn set_led_mode(&self, value: AuraModeNum) -> zbus::Result<()>;

    /// LedModeData property
    #[zbus(property)]
    fn led_mode_data(&self) -> zbus::Result<AuraEffect>;
    #[zbus(property)]
    fn set_led_mode_data(&self, value: AuraEffect) -> zbus::Result<()>;

    /// LedPower property
    #[zbus(property)]
    fn led_power(&self) -> zbus::Result<LaptopAuraPower>;
    #[zbus(property)]
    fn set_led_power(&self, value: LaptopAuraPower) -> zbus::Result<()>;

    /// SupportedBrightness property
    #[zbus(property)]
    fn supported_brightness(&self) -> zbus::Result<Vec<LedBrightness>>;

    /// SupportedBasicModes property
    #[zbus(property)]
    fn supported_basic_modes(&self) -> zbus::Result<Vec<AuraModeNum>>;

    /// SupportedBasicZones property
    #[zbus(property)]
    fn supported_basic_zones(&self) -> zbus::Result<Vec<AuraZone>>;

    /// SupportedPowerZones property
    #[zbus(property)]
    fn supported_power_zones(&self) -> zbus::Result<Vec<PowerZones>>;
}

pub struct AuraProxyPerkey<'a>(AuraProxyBlocking<'a>);

impl<'a> AuraProxyPerkey<'a> {
    #[inline]
    pub fn new(conn: &Connection) -> Result<Self> {
        Ok(AuraProxyPerkey(AuraProxyBlocking::new(conn)?))
    }

    #[inline]
    pub fn proxy(&self) -> &AuraProxyBlocking<'a> {
        &self.0
    }

    /// Write a single colour block.
    ///
    /// Intentionally blocks for 10ms after sending to allow the block to
    /// be written to the keyboard EC. This should not be async.
    #[inline]
    pub fn direct_addressing_raw(&self, direct_raw: UsbPackets) -> Result<()> {
        self.0.direct_addressing_raw(direct_raw)?;
        std::thread::sleep(std::time::Duration::from_millis(BLOCKING_TIME));
        Ok(())
    }
}
