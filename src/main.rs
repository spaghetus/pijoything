use std::{
	ops::Deref,
	path::PathBuf,
	sync::{Arc, Mutex, RwLock},
	time::{Duration, Instant},
};

use evdev::{
	uinput::{VirtualDevice, VirtualDeviceBuilder},
	AttributeSet, BusType, EventType, InputEvent, InputId, Key,
};
use rppal::gpio::{Gpio, InputPin};
use serde::{Deserialize, Serialize};
use signal::{trap::Trap, Signal};
use structopt::StructOpt;

const KEYS: [evdev::Key; 8] = [
	Key::BTN_DPAD_UP,
	Key::BTN_DPAD_LEFT,
	Key::BTN_DPAD_DOWN,
	Key::BTN_DPAD_RIGHT,
	Key::BTN_EAST,
	Key::BTN_SOUTH,
	Key::BTN_START,
	Key::BTN_SELECT,
];

lazy_static::lazy_static! {
	static ref SUPPORTED_KEYS: AttributeSet<Key> = {
		let mut keys = AttributeSet::new();
		for key in KEYS {
			keys.insert(key);
		}
		keys
	};
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Joystick {
	pub buttons: [u8; 8],
	#[serde(skip)]
	vdevice: Option<Arc<Mutex<VirtualDevice>>>,
	#[serde(skip)]
	gpio: Option<Gpio>,
	#[serde(skip)]
	pins: Vec<Arc<Mutex<InputPin>>>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct JoystickState {
	pub buttons: [bool; 8],
}

impl Joystick {
	fn deinit(&mut self) {
		for pin in &self.pins {
			pin.lock().unwrap().clear_async_interrupt().unwrap_or(())
		}
		self.pins.clear();
		self.gpio = None;
		self.vdevice = None;
	}

	fn init(&mut self) {
		self.gpio = Some(Gpio::new().expect("Failed to lock gpio"));
		self.vdevice = Some(Arc::new(Mutex::new(
			VirtualDeviceBuilder::new()
				.expect("Failed to start building vdevice")
				.name("PiJoyThing")
				.with_keys(&SUPPORTED_KEYS)
				.expect("Failed to write keys")
				.build()
				.expect("Failed to finalize vdevice"),
		)));
		let pins: Vec<Arc<Mutex<InputPin>>> = self
			.buttons
			.iter()
			.map(|index| self.gpio.as_ref().unwrap().get(*index).expect("Bad pin"))
			.map(|v| v.into_input_pullup())
			.map(Mutex::new)
			.map(Arc::new)
			.collect();
		for (n, pin) in pins.iter().enumerate() {
			let vdev = self.vdevice.as_ref().unwrap().clone();
			pin.lock()
				.unwrap()
				.set_async_interrupt(rppal::gpio::Trigger::Both, move |level| {
					vdev.lock()
						.unwrap()
						.emit(&[InputEvent::new_now(
							EventType::KEY,
							KEYS[n].code(),
							match level {
								rppal::gpio::Level::Low => 1,
								rppal::gpio::Level::High => 0,
							},
						)])
						.expect("Failed to write messages");
				})
				.expect("Failed to set interrupt");
		}
		self.pins = pins;
	}
}

#[derive(StructOpt)]
struct Opts {
	#[structopt(parse(from_os_str))]
	pub config: PathBuf,
}

fn main() {
	let opts = Opts::from_args();
	let j: String = std::fs::read_to_string(opts.config).expect("Failed to read config");
	let mut j: Joystick = toml::from_str(&j).expect("Failed to deser config");
	j.init();
	let trap = Trap::trap(&[Signal::SIGINT]);
	while trap
		.wait(Instant::now() + Duration::from_secs(3000))
		.is_none()
	{}
	j.deinit();
	println!("Exited");
}
