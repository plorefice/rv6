//! Virtio input device driver.

use alloc::{sync::Arc, vec::Vec};
use fdt::Node;

use crate::{
    drivers::{
        DriverError,
        virtio::{
            InterruptStatus, Status, VirtioDev, VirtioDriver,
            virtq::{Virtq, VirtqBuffer},
        },
    },
    irq::{self, IrqHandler, IrqReturn},
    mm::dma::{DmaDirection, DmaObject},
    sync::IrqSpinLock,
};

pub struct VirtioInputDev<D> {
    dev: D,
    eventq: IrqSpinLock<Virtq>,
    pending: IrqSpinLock<Vec<Option<EventBuf>>>,
}

struct EventBuf {
    dma_obj: DmaObject<'static, VirtioInputEvent>,
}

impl<D> VirtioInputDev<D>
where
    D: VirtioDev,
{
    pub fn new(dev: D, node: &Node) -> Result<Arc<Self>, DriverError<'static>> {
        // Recognize the device
        dev.update_status(Status::ACKNOWLEDGE);
        dev.update_status(Status::DRIVER);

        // Acknowledge the device features (none for now)
        dev.enable_device_features(0, 0);

        // Configure virtqueues
        let eventq = dev.allocate_virtq(0);
        let _ = dev.allocate_virtq(1); // required by QEMU but not used by the driver

        // Initialize the pending buffer list
        let mut pending = Vec::with_capacity(eventq.size() as usize);
        for _ in 0..eventq.size() {
            pending.push(None);
        }

        let slf = Arc::new(Self {
            dev,
            eventq: IrqSpinLock::new(eventq),
            pending: IrqSpinLock::new(pending),
        });

        // Preallocate and submit buffers for the event queue
        slf.allocate_event_buffers()?;

        // Device is now live
        slf.dev.update_status(Status::DRIVER_OK);

        // Register interrupt handler
        let irq = node
            .property::<u32>("interrupts")
            .ok_or(DriverError::MissingRequiredProperty("interrupts"))?;

        irq::request_irq(irq, slf.clone());

        Ok(slf)
    }

    fn allocate_event_buffers(&self) -> Result<(), DriverError<'static>> {
        let mut virtq = self.eventq.lock();
        let mut pending = self.pending.lock();

        for _ in 0..virtq.size() {
            let dma_obj = self.dev.allocate_guest_mem(VirtioInputEvent::default())?;
            let head = virtq.submit(
                &self.dev,
                [&VirtqBuffer::Writeable {
                    addr: dma_obj.dma_addr(),
                    len: dma_obj.size(),
                }],
            );
            pending[head as usize] = Some(EventBuf { dma_obj });
        }

        Ok(())
    }

    fn handle_event(&self, event: &VirtioInputEvent) {
        if event.event_type != EventType::Key as u16 || event.value != 1 {
            return; // only handle key press events for now
        }

        let code = event.code;
        kprintln!("Virtio input event: code={code}");
    }
}

impl<D> VirtioDriver for VirtioInputDev<D> {}

impl<D> IrqHandler for VirtioInputDev<D>
where
    D: VirtioDev,
{
    fn handle(&self) -> IrqReturn {
        let irq_status = self.dev.interrupts();

        if irq_status.contains(InterruptStatus::USED_BUFFER) {
            self.dev.clear_interrupts(InterruptStatus::USED_BUFFER);

            let mut virtq = self.eventq.lock();
            let mut pending = self.pending.lock();

            while let Some(c) = virtq.pop_used() {
                let buf = pending[c.head as usize].take().unwrap();
                if c.written >= 8 {
                    buf.dma_obj.sync_for_cpu(DmaDirection::ToDevice);
                    self.handle_event(buf.dma_obj.as_ref());
                }
                buf.dma_obj.sync_for_device(DmaDirection::ToDevice);
                let head = virtq.submit(
                    &self.dev,
                    [&VirtqBuffer::Writeable {
                        addr: buf.dma_obj.dma_addr(),
                        len: buf.dma_obj.size(),
                    }],
                );
                pending[head as usize] = Some(buf);
            }

            IrqReturn::Handled
        } else if irq_status.is_empty() {
            IrqReturn::Handled // spurious interrupt, ignore
        } else {
            IrqReturn::Unhandled
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C, packed)]
struct VirtioInputEvent {
    event_type: u16, // event type (e.g., EV_KEY, EV_REL, etc.)
    code: u16,       // event code (e.g., key code, axis code, etc.)
    value: u32,      // event value (e.g., key press/release, axis movement, etc.)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventType {
    Key = 0x01,
}

#[allow(unused)]
enum KeyCode {
    KeyReserved = 0,
    KeyEsc = 1,
    Key1 = 2,
    Key2 = 3,
    Key3 = 4,
    Key4 = 5,
    Key5 = 6,
    Key6 = 7,
    Key7 = 8,
    Key8 = 9,
    Key9 = 10,
    Key0 = 11,
    KeyMinus = 12,
    KeyEqual = 13,
    KeyBackspace = 14,
    KeyTab = 15,
    KeyQ = 16,
    KeyW = 17,
    KeyE = 18,
    KeyR = 19,
    KeyT = 20,
    KeyY = 21,
    KeyU = 22,
    KeyI = 23,
    KeyO = 24,
    KeyP = 25,
    KeyLeftBrace = 26,
    KeyRightBrace = 27,
    KeyEnter = 28,
    KeyLeftCtrl = 29,
    KeyA = 30,
    KeyS = 31,
    KeyD = 32,
    KeyF = 33,
    KeyG = 34,
    KeyH = 35,
    KeyJ = 36,
    KeyK = 37,
    KeyL = 38,
    KeySemicolon = 39,
    KeyApostrophe = 40,
    KeyGrave = 41,
    KeyLeftShift = 42,
    KeyBackslash = 43,
    KeyZ = 44,
    KeyX = 45,
    KeyC = 46,
    KeyV = 47,
    KeyB = 48,
    KeyN = 49,
    KeyM = 50,
    KeyComma = 51,
    KeyDot = 52,
    KeySlash = 53,
    KeyRightShift = 54,
    KeyKpAsterisk = 55,
    KeyLeftAlt = 56,
    KeySpace = 57,
    KeyCapsLock = 58,
    KeyF1 = 59,
    KeyF2 = 60,
    KeyF3 = 61,
    KeyF4 = 62,
    KeyF5 = 63,
    KeyF6 = 64,
    KeyF7 = 65,
    KeyF8 = 66,
    KeyF9 = 67,
    KeyF10 = 68,
    KeyNumLock = 69,
    KeyScrollLock = 70,
    KeyKp7 = 71,
    KeyKp8 = 72,
    KeyKp9 = 73,
    KeyKpMinus = 74,
    KeyKp4 = 75,
    KeyKp5 = 76,
    KeyKp6 = 77,
    KeyKpPlus = 78,
    KeyKp1 = 79,
    KeyKp2 = 80,
    KeyKp3 = 81,
    KeyKp0 = 82,
    KeyKpDot = 83,
    KeyZenkakuHankaku = 85,
    Key102nd = 86,
    KeyF11 = 87,
    KeyF12 = 88,
    KeyRo = 89,
    KeyKatakana = 90,
    KeyHiragana = 91,
    KeyHenkan = 92,
    KeyKatakanaHiragana = 93,
    KeyMuhenkan = 94,
    KeyKpJpComma = 95,
    KeyKpEnter = 96,
    KeyRightCtrl = 97,
    KeyKpSlash = 98,
    KeySysRq = 99,
    KeyRightAlt = 100,
    KeyLineFeed = 101,
    KeyHome = 102,
    KeyUp = 103,
    KeyPageUp = 104,
    KeyLeft = 105,
    KeyRight = 106,
    KeyEnd = 107,
    KeyDown = 108,
    KeyPageDown = 109,
    KeyInsert = 110,
    KeyDelete = 111,
    KeyMacro = 112,
    KeyMute = 113,
    KeyVolumeDown = 114,
    KeyVolumeUp = 115,
    KeyPower = 116, /* SC System Power Down */
    KeyKpEqual = 117,
    KeyKpPlusMinus = 118,
    KeyPause = 119,
    KeyScale = 120, /* AL Compiz Scale (Expose) */
    KeyKpComma = 121,
    KeyHangeul = 122,
    KeyHanja = 123,
    KeyYen = 124,
    KeyLeftMeta = 125,
    KeyRightMeta = 126,
    KeyCompose = 127,
    KeyStop = 128, /* AC Stop */
    KeyAgain = 129,
    KeyProps = 130, /* AC Properties */
    KeyUndo = 131,  /* AC Undo */
    KeyFront = 132,
    KeyCopy = 133,  /* AC Copy */
    KeyOpen = 134,  /* AC Open */
    KeyPaste = 135, /* AC Paste */
    KeyFind = 136,  /* AC Search */
    KeyCut = 137,   /* AC Cut */
    KeyHelp = 138,  /* AL Integrated Help Center */
    KeyMenu = 139,  /* Menu (show menu) */
    KeyCalc = 140,  /* AL Calculator */
    KeySetup = 141,
    KeySleep = 142,  /* SC System Sleep */
    KeyWakeUp = 143, /* System Wake Up */
    KeyFile = 144,   /* AL Local Machine Browser */
    KeySendFile = 145,
    KeyDeleteFile = 146,
    KeyXfer = 147,
    KeyProg1 = 148,
    KeyProg2 = 149,
    KeyWww = 150, /* AL Internet Browser */
    KeyMsdos = 151,
    KeyCoffee = 152,        /* AL Terminal Lock/Screensaver */
    KeyRotateDisplay = 153, /* Display orientation for e.g. tablets */
    KeyCycleWindows = 154,
    KeyMail = 155,
    KeyBookmarks = 156, /* AC Bookmarks */
    KeyComputer = 157,
    KeyBack = 158,    /* AC Back */
    KeyForward = 159, /* AC Forward */
    KeyCloseCd = 160,
    KeyEjectCd = 161,
    KeyEjectCloseCd = 162,
    KeyNextSong = 163,
    KeyPlayPause = 164,
    KeyPreviousSong = 165,
    KeyStopCd = 166,
    KeyRecord = 167,
    KeyRewind = 168,
    KeyPhone = 169, /* Media Select Telephone */
    KeyIso = 170,
    KeyConfig = 171,   /* AL Consumer Control Configuration */
    KeyHomepage = 172, /* AC Home */
    KeyRefresh = 173,  /* AC Refresh */
    KeyExit = 174,     /* AC Exit */
    KeyMove = 175,
    KeyEdit = 176,
    KeyScrollUp = 177,
    KeyScrollDown = 178,
    KeyKpLeftParen = 179,
    KeyKpRightParen = 180,
    KeyNew = 181,  /* AC New */
    KeyRedo = 182, /* AC Redo/Repeat */
    KeyF13 = 183,
    KeyF14 = 184,
    KeyF15 = 185,
    KeyF16 = 186,
    KeyF17 = 187,
    KeyF18 = 188,
    KeyF19 = 189,
    KeyF20 = 190,
    KeyF21 = 191,
    KeyF22 = 192,
    KeyF23 = 193,
    KeyF24 = 194,
    KeyPlayCd = 200,
    KeyPauseCd = 201,
    KeyProg3 = 202,
    KeyProg4 = 203,
    KeyAllApplications = 204, /* AC Desktop Show All Applications */
    KeySuspend = 205,
    KeyClose = 206, /* AC Close */
    KeyPlay = 207,
    KeyFastForward = 208,
    KeyBassBoost = 209,
    KeyPrint = 210, /* AC Print */
    KeyHp = 211,
    KeyCamera = 212,
    KeySound = 213,
    KeyQuestion = 214,
    KeyEmail = 215,
    KeyChat = 216,
    KeySearch = 217,
    KeyConnect = 218,
    KeyFinance = 219, /* AL Checkbook/Finance */
    KeySport = 220,
    KeyShop = 221,
    KeyAlterase = 222,
    KeyCancel = 223, /* AC Cancel */
    KeyBrightnessDown = 224,
    KeyBrightnessUp = 225,
    KeyMedia = 226,
    KeySwitchVideoMode = 227, /* Cycle between available video outputs (Monitor/LCD/TV-out/etc) */
    KeyKbdIllumToggle = 228,
    KeyKbdIllumDown = 229,
    KeyKbdIllumUp = 230,
    KeySend = 231,        /* AC Send */
    KeyReply = 232,       /* AC Reply */
    KeyForwardMail = 233, /* AC Forward Msg */
    KeySave = 234,        /* AC Save */
    KeyDocuments = 235,
    KeyBattery = 236,
    KeyBluetooth = 237,
    KeyWlan = 238,
    KeyUwb = 239,
    KeyUnknown = 240,
    KeyVideoNext = 241,       /* drive next video source */
    KeyVideoPrev = 242,       /* drive previous video source */
    KeyBrightnessCycle = 243, /* brightness up, after max is min */
    KeyBrightnessAuto = 244, /* Set Auto Brightness: manual brightness control is off, rely on ambient */
    KeyDisplayOff = 245,     /* display device to off state */
    KeyWwan = 246,           /* Wireless WAN (LTE, UMTS, GSM, etc.) */
    KeyRfkill = 247,         /* Key that controls all radios */
    KeyMicMute = 248,        /* Mute / unmute the microphone */
}
