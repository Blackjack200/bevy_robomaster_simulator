mod exporter;
mod frame_data;

pub use exporter::*;
pub use frame_data::*;

use bevy::prelude::*;

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TelemetryPipeline>()
            .add_systems(PostUpdate, telemetry_dispatch);
    }
}

#[derive(Resource, Default)]
pub struct TelemetryPipeline {
    exporters: Vec<Box<dyn TelemetryExporter + Send + Sync>>,
}

impl TelemetryPipeline {
    pub fn add_exporter<E: TelemetryExporter + Send + Sync + 'static>(&mut self, exporter: E) {
        self.exporters.push(Box::new(exporter));
    }

    pub fn dispatch(&mut self, data: &FrameData) {
        for exporter in &mut self.exporters {
            exporter.on_frame(data);
        }
    }

    pub fn exporter_count(&self) -> usize {
        self.exporters.len()
    }
}

fn telemetry_dispatch(mut pipeline: ResMut<TelemetryPipeline>, time: Res<Time>) {
    if pipeline.exporter_count() == 0 {
        return;
    }

    // Create frame data from current state
    let frame_data = FrameData {
        timestamp: time.elapsed_secs_f64(),
        armors: Vec::new(),
        poses: std::collections::HashMap::new(),
    };

    pipeline.dispatch(&frame_data);
}
