// Compiles only the production device-selection functions against a fake host.
// No CPAL stream, OS device or audio callback is used.
#[cfg(test)]
mod source_meter_tests {
 use anyhow::{anyhow,Result};
 type LevelMap=();
 mod cpal {
  pub struct Host;
  pub struct Device(pub &'static str);
  pub struct SupportedStreamConfig;
  pub struct Stream(pub &'static str);
  pub fn default_host()->Host{Host}
  impl Host {
   pub fn default_input_device(&self)->Option<Device>{Some(Device("default microphone"))}
   pub fn default_output_device(&self)->Option<Device>{Some(Device("default system"))}
   pub fn input_devices(&self)->anyhow::Result<std::vec::IntoIter<Device>>{Ok(vec![Device("shared name")].into_iter())}
   pub fn output_devices(&self)->anyhow::Result<std::vec::IntoIter<Device>>{Ok(vec![Device("shared name")].into_iter())}
  }
  impl Device {
   pub fn name(&self)->anyhow::Result<String>{Ok(self.0.into())}
   pub fn default_input_config(&self)->anyhow::Result<SupportedStreamConfig>{Ok(SupportedStreamConfig)}
   pub fn default_output_config(&self)->anyhow::Result<SupportedStreamConfig>{Ok(SupportedStreamConfig)}
  }
 }
 fn build_input_level_stream(_: &cpal::Device,_:cpal::SupportedStreamConfig,_:String,kind:&'static str,_:LevelMap)->Result<cpal::Stream>{Ok(cpal::Stream(kind))}
 %METER_SELECTION%
 #[test]
 fn a_chevron_preview_resolves_only_its_own_default_source() {
  assert_eq!(default_monitor_devices(Some("input")),vec!["default microphone"]);
  assert_eq!(default_monitor_devices(Some("output")),vec!["default system"]);
  assert_eq!(default_monitor_devices(None),vec!["default microphone","default system"]);
 }
 #[test]
 fn shared_device_names_do_not_cross_source_types() {
  assert_eq!(open_level_stream("shared name",(),Some("input")).unwrap().0,"input");
  assert_eq!(open_level_stream("shared name",(),Some("output")).unwrap().0,"output");
 }
}
