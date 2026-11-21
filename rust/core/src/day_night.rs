use std::f64::consts::PI;

pub const DAY_LENGTH_SEC: f64 = 480.0;

#[derive(Clone, Debug)]
pub struct DayNightSystem {
    pub time: f64,
    pub day: i32,
    pub day_length: f64,
    pub is_night: bool,
}

impl Default for DayNightSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl DayNightSystem {
    pub fn new() -> Self {
        Self {
            time: 0.28,
            day: 1,
            day_length: DAY_LENGTH_SEC,
            is_night: false,
        }
    }

    pub fn update(&mut self, dt: f64) {
        self.time += dt / self.day_length;
        if self.time >= 1.0 {
            self.time -= 1.0;
            self.day += 1;
        }
        let ang = (self.time - 0.25) * PI * 2.0;
        let sy = ang.sin();
        self.is_night = sy < -0.06;
    }
}
