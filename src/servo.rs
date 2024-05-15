use std::time::Duration;

use rppal::gpio;
use tokio::sync::mpsc;

pub enum ServoPosition {
    Left,
    Center,
    Right,
    Custom(f32),
}

const GPIO_PWM: u8 = 18;
const PERIOD_MS: u64 = 20;
const MAX_PULSE_WIDTH_US: u64 = 2000;
const MIN_PULSE_WIDTH_US: u64 = 1000;

pub fn initilize_servo() -> gpio::Result<gpio::OutputPin> {
    let mut pin = gpio::Gpio::new()?.get(GPIO_PWM)?.into_output();
    pin.set_pwm(
        Duration::from_millis(PERIOD_MS),
        Duration::from_micros((MIN_PULSE_WIDTH_US + MAX_PULSE_WIDTH_US) / 2),
    )?;
    Ok(pin)
}

pub async fn servo_task(
    mut servo_rx: mpsc::UnboundedReceiver<ServoPosition>,
    mut servo_pwm: gpio::OutputPin,
) {
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
                .set_pwm(
                    Duration::from_millis(PERIOD_MS),
                    Duration::from_micros(pulse_width),
                )
                .unwrap();
        }
    }
}
