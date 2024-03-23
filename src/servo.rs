use std::time::Duration;

use rppal::pwm;
use tokio::sync::mpsc;

pub enum ServoPosition {
    Left,
    Center,
    Right,
    Custom(f32),
}

const MAX_PULSE_WIDTH_US: u64 = 2000;
const MIN_PULSE_WIDTH_US: u64 = 1000;

pub fn initilize_servo() -> pwm::Result<pwm::Pwm> {
    pwm::Pwm::with_period(
        pwm::Channel::Pwm0,
        Duration::from_millis(20),
        Duration::from_micros((MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) / 2),
        pwm::Polarity::Normal,
        true,
    )
}

pub async fn servo_task(mut servo_rx: mpsc::UnboundedReceiver<ServoPosition>, servo_pwm: pwm::Pwm) {
    loop {
        while let Some(servo_pos) = servo_rx.recv().await {
            let pulse_width = match servo_pos {
                ServoPosition::Left => MAX_PULSE_WIDTH_US,
                ServoPosition::Center => (MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) / 2,
                ServoPosition::Right => MIN_PULSE_WIDTH_US,
                ServoPosition::Custom(pos) => {
                    ((pos * (MAX_PULSE_WIDTH_US - MIN_PULSE_WIDTH_US) as f32 / 2.)
                        + (MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) as f32 / 2.)
                        as u64
                }
            };
            servo_pwm
                .set_pulse_width(Duration::from_micros(pulse_width))
                .unwrap();
        }
    }
}
