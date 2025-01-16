use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use ::zbus::export::futures_util::lock::Mutex;
use config_traits::StdConfig;
use log::{debug, error, info};
use rog_platform::asus_armoury::{AttrValue, Attribute, FirmwareAttribute, FirmwareAttributes};
use rog_platform::platform::{RogPlatform, ThrottlePolicy};
use serde::{Deserialize, Serialize};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Type, Value};
use zbus::{fdo, interface, Connection};

use crate::config::Config;
use crate::error::RogError;
use crate::{Reloadable, ASUS_ZBUS_PATH};

const MOD_NAME: &str = "asus_armoury";

#[derive(Debug, Default, Clone, Deserialize, Serialize, Type, Value, OwnedValue)]
pub struct PossibleValues {
    strings: Vec<String>,
    nums: Vec<i32>
}

fn dbus_path_for_attr(attr_name: &str) -> OwnedObjectPath {
    ObjectPath::from_str_unchecked(&format!("{ASUS_ZBUS_PATH}/{MOD_NAME}/{attr_name}")).into()
}

#[derive(Clone)]
pub struct AsusArmouryAttribute {
    attr: Attribute,
    config: Arc<Mutex<Config>>,
    /// platform control required here for access to PPD or Throttle profile
    platform: RogPlatform
}

impl AsusArmouryAttribute {
    pub fn new(attr: Attribute, platform: RogPlatform, config: Arc<Mutex<Config>>) -> Self {
        Self {
            attr,
            config,
            platform
        }
    }

    pub async fn move_to_zbus(self, connection: &Connection) -> Result<(), RogError> {
        let path = dbus_path_for_attr(self.attr.name());
        connection
            .object_server()
            .at(path.clone(), self)
            .await
            .map_err(|e| error!("Couldn't add server at path: {path}, {e:?}"))
            .ok();
        Ok(())
    }

    async fn watch_and_notify(
        &mut self,
        signal_ctxt: SignalEmitter<'static>
    ) -> Result<(), RogError> {
        use zbus::export::futures_util::StreamExt;

        let ctrl = self.clone();
        let name = self.name();
        match self.attr.get_watcher() {
            Ok(watch) => {
                let name = <&str>::from(name);
                tokio::spawn(async move {
                    let mut buffer = [0; 32];
                    watch
                        .into_event_stream(&mut buffer)
                        .unwrap()
                        .for_each(|_| async {
                            debug!("{} changed", name);
                            ctrl.current_value_changed(&signal_ctxt).await.ok();
                        })
                        .await;
                });
            }
            Err(e) => info!(
                "inotify watch failed: {}. You can ignore this if your device does not support \
                 the feature",
                e
            )
        }

        Ok(())
    }
}

impl crate::Reloadable for AsusArmouryAttribute {
    async fn reload(&mut self) -> Result<(), RogError> {
        info!("Reloading {}", self.attr.name());
        let profile: ThrottlePolicy =
            ThrottlePolicy::from_str(self.platform.get_platform_profile()?.as_str())?;
        if let Some(tunings) = self.config.lock().await.profile_tunings.get(&profile) {
            if let Some(tune) = tunings.get(&self.name()) {
                self.attr
                    .set_current_value(AttrValue::Integer(*tune))
                    .map_err(|e| {
                        error!("Could not set value: {e:?}");
                        e
                    })?;
                info!("Set {} to {:?}", self.attr.name(), tune);
            }
        }

        Ok(())
    }
}

/// If return is `-1` on a property then there is avilable value for that
/// property
#[interface(name = "xyz.ljones.AsusArmoury")]
impl AsusArmouryAttribute {
    #[zbus(property)]
    fn name(&self) -> FirmwareAttribute {
        self.attr.name().into()
    }

    #[zbus(property)]
    async fn available_attrs(&self) -> Vec<String> {
        let mut attrs = Vec::new();
        if !matches!(self.attr.default_value(), AttrValue::None) {
            attrs.push("default_value".to_string());
        }
        if !matches!(self.attr.min_value(), AttrValue::None) {
            attrs.push("min_value".to_string());
        }
        if !matches!(self.attr.max_value(), AttrValue::None) {
            attrs.push("max_value".to_string());
        }
        if !matches!(self.attr.scalar_increment(), AttrValue::None) {
            attrs.push("scalar_increment".to_string());
        }
        if !matches!(self.attr.possible_values(), AttrValue::None) {
            attrs.push("possible_values".to_string());
        }
        // TODO: Don't unwrap, use error
        if let Ok(value) = self.attr.current_value().map_err(|e| {
            error!("Failed to read: {e:?}");
            e
        }) {
            if !matches!(value, AttrValue::None) {
                attrs.push("current_value".to_string());
            }
        }
        attrs
    }

    /// If return is `-1` then there is no default value
    #[zbus(property)]
    async fn default_value(&self) -> i32 {
        match self.attr.default_value() {
            AttrValue::Integer(i) => *i,
            _ => -1
        }
    }

    #[zbus(property)]
    async fn min_value(&self) -> i32 {
        match self.attr.min_value() {
            AttrValue::Integer(i) => *i,
            _ => -1
        }
    }

    #[zbus(property)]
    async fn max_value(&self) -> i32 {
        match self.attr.max_value() {
            AttrValue::Integer(i) => *i,
            _ => -1
        }
    }

    #[zbus(property)]
    async fn scalar_increment(&self) -> i32 {
        match self.attr.scalar_increment() {
            AttrValue::Integer(i) => *i,
            _ => -1
        }
    }

    #[zbus(property)]
    async fn possible_values(&self) -> Vec<i32> {
        match self.attr.possible_values() {
            AttrValue::EnumInt(i) => i.clone(),
            _ => Vec::default()
        }
    }

    #[zbus(property)]
    async fn current_value(&self) -> fdo::Result<i32> {
        if let Ok(AttrValue::Integer(i)) = self.attr.current_value() {
            return Ok(i);
        }
        Err(fdo::Error::Failed(
            "Could not read current value".to_string()
        ))
    }

    #[zbus(property)]
    async fn set_current_value(&mut self, value: i32) -> fdo::Result<()> {
        self.attr
            .set_current_value(AttrValue::Integer(value))
            .map_err(|e| {
                error!("Could not set value: {e:?}");
                e
            })?;

        if matches!(
            self.name(),
            FirmwareAttribute::PptPl1Spl
                | FirmwareAttribute::PptPl2Sppt
                | FirmwareAttribute::PptPl3Fppt
                | FirmwareAttribute::PptFppt
                | FirmwareAttribute::PptApuSppt
                | FirmwareAttribute::PptPlatformSppt
                | FirmwareAttribute::NvDynamicBoost
                | FirmwareAttribute::NvTempTarget
                | FirmwareAttribute::DgpuBaseTgp
                | FirmwareAttribute::DgpuTgp
        ) {
            let profile: ThrottlePolicy =
                ThrottlePolicy::from_str(self.platform.get_platform_profile()?.as_str())?;

            // var here to prevent async deadlock on else clause
            let has_profile = self
                .config
                .lock()
                .await
                .profile_tunings
                .contains_key(&profile);
            if has_profile {
                if let Some(tunings) = self.config.lock().await.profile_tunings.get_mut(&profile) {
                    if let Some(tune) = tunings.get_mut(&self.name()) {
                        *tune = value;
                    } else {
                        tunings.insert(self.name(), value);
                        debug!("Set tuning config for {} = {:?}", self.attr.name(), value);
                    }
                }
            } else {
                debug!("Adding tuning config for {}", profile);
                self.config
                    .lock()
                    .await
                    .profile_tunings
                    .insert(profile, HashMap::from([(self.name(), value)]));
                debug!("Set tuning config for {} = {:?}", self.attr.name(), value);
            }
        } else {
            let has_attr = self
                .config
                .lock()
                .await
                .armoury_settings
                .contains_key(&self.name());
            if has_attr {
                if let Some(setting) = self
                    .config
                    .lock()
                    .await
                    .armoury_settings
                    .get_mut(&self.name())
                {
                    *setting = value
                }
            } else {
                debug!("Adding config for {}", self.attr.name());
                self.config
                    .lock()
                    .await
                    .armoury_settings
                    .insert(self.name(), value);
                debug!("Set config for {} = {:?}", self.attr.name(), value);
            }
        }
        self.config.lock().await.write();
        Ok(())
    }
}

pub async fn start_attributes_zbus(
    conn: &Connection,
    platform: RogPlatform,
    config: Arc<Mutex<Config>>
) -> Result<(), RogError> {
    for attr in FirmwareAttributes::new().attributes() {
        let mut attr = AsusArmouryAttribute::new(attr.clone(), platform.clone(), config.clone());
        attr.reload().await?;

        let path = dbus_path_for_attr(attr.attr.name());
        let sig = zbus::object_server::SignalEmitter::new(conn, path)?;
        attr.watch_and_notify(sig).await?;

        attr.move_to_zbus(conn).await?;
    }
    Ok(())
}
