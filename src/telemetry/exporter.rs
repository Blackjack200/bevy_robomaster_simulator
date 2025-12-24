use super::FrameData;

pub trait TelemetryExporter {
    fn on_frame(&mut self, data: &FrameData);
    fn name(&self) -> &'static str;
}
