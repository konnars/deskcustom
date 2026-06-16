use deskcustom_config::MouseProfile;

#[derive(Debug, Clone)]
pub struct SmoothedMove {
    pub dx: i16,
    pub dy: i16,
}

pub struct MousePipeline {
    profile: MouseProfile,
    ewma_x: f32,
    ewma_y: f32,
    pending_dx: f32,
    pending_dy: f32,
    last_emit: Option<std::time::Instant>,
    min_interval: std::time::Duration,
    seq: u32,
}

impl MousePipeline {
    pub fn new(profile: MouseProfile) -> Self {
        let hz = profile.poll_rate_cap_hz.max(1);
        Self {
            profile,
            ewma_x: 0.0,
            ewma_y: 0.0,
            pending_dx: 0.0,
            pending_dy: 0.0,
            last_emit: None,
            min_interval: std::time::Duration::from_micros(1_000_000 / hz as u64),
            seq: 0,
        }
    }

    pub fn update_profile(&mut self, profile: MouseProfile) {
        let hz = profile.poll_rate_cap_hz.max(1);
        self.min_interval = std::time::Duration::from_micros(1_000_000 / hz as u64);
        self.profile = profile;
    }

    /// Ingest raw delta from network or capture. Returns packet ready to send/inject.
    pub fn ingest(&mut self, dx: i32, dy: i32) -> Option<SmoothedMove> {
        let scale = self.profile.dpi_scale;
        let mut x = dx as f32 * scale;
        let mut y = dy as f32 * scale;

        if self.profile.smoothing == "ewma" {
            let a = self.profile.ewma_alpha.clamp(0.01, 1.0);
            self.ewma_x = a * x + (1.0 - a) * self.ewma_x;
            self.ewma_y = a * y + (1.0 - a) * self.ewma_y;
            x = self.ewma_x;
            y = self.ewma_y;
        }

        self.pending_dx += x;
        self.pending_dy += y;

        let now = std::time::Instant::now();
        let coalesce = std::time::Duration::from_micros(self.profile.coalesce_us as u64);

        if let Some(last) = self.last_emit {
            if now.duration_since(last) < coalesce {
                return None;
            }
            if now.duration_since(last) < self.min_interval {
                return None;
            }
        }

        let out_dx = self.pending_dx.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let out_dy = self.pending_dy.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;

        if out_dx == 0 && out_dy == 0 {
            return None;
        }

        self.pending_dx -= out_dx as f32;
        self.pending_dy -= out_dy as f32;
        self.last_emit = Some(now);
        self.seq = self.seq.wrapping_add(1);

        Some(SmoothedMove {
            dx: out_dx,
            dy: out_dy,
        })
    }

    pub fn next_seq(&self) -> u32 {
        self.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_emits() {
        let mut pipe = MousePipeline::new(MouseProfile {
            dpi_scale: 2.0,
            smoothing: "none".into(),
            coalesce_us: 0,
            ..MouseProfile::default()
        });
        let mv = pipe.ingest(3, 4).expect("move");
        assert_eq!(mv.dx, 6);
        assert_eq!(mv.dy, 8);
    }
}
