//! Bounded input policy shared by the HID backend and native UI.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Output {
    pub kind: u32,
    pub code: u32,
    pub modifiers: u32,
    pub x: f64,
    pub y: f64,
}
// Output kinds: 1 shortcut, 2 mouse down/up (x=1/0), 3 scroll pixels,
// 4 media key, 5 toggle wheel (handled on the device worker).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Action {
    pub kind: u32,
    pub code: u32,
    pub modifiers: u32,
}
impl Action {
    fn decode(words: &[u32]) -> Result<Self, String> {
        let a = Self {
            kind: words[0],
            code: words[1],
            modifiers: words[2],
        };
        let valid = match a.kind {
            0 => a.code == 0 && a.modifiers == 0,
            1 => a.code <= 127 && a.modifiers <= 15,
            2 => a.code <= 4 && a.modifiers == 0,
            4 => [0, 1, 7, 16, 17, 18].contains(&a.code) && a.modifiers == 0,
            5 => a.code == 0 && a.modifiers == 0,
            _ => false,
        };
        if !valid {
            return Err("Invalid action or shortcut".into());
        }
        Ok(a)
    }
    pub fn output(self, pressed: bool) -> Option<Output> {
        if self.kind == 0 || (!pressed && self.kind != 2) {
            return None;
        }
        Some(Output {
            kind: self.kind,
            code: self.code,
            modifiers: self.modifiers,
            x: if pressed { 1.0 } else { 0.0 },
            y: 0.0,
        })
    }
}
pub const CONTROL_IDS: [u16; 5] = [0x53, 0x56, 0x52, 0xc4, 0xc3];
#[derive(Clone, Copy, Debug, Default)]
pub struct Binding {
    pub enabled: bool,
    pub gesture: bool,
    pub actions: [Action; 5],
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub dpi: u16,
    pub wheel_mode: u8,
    pub threshold: u8,
    pub vertical: bool,
    pub vertical_gain: f64,
    pub vertical_invert: bool,
    pub smooth: bool,
    pub horizontal: bool,
    pub horizontal_gain: f64,
    pub horizontal_invert: bool,
    pub horizontal_actions: [Action; 2],
    pub gesture_threshold: i32,
    pub bindings: [Binding; 5],
}
impl Settings {
    pub const WORDS: usize = 105;
    pub fn decode(w: &[u32]) -> Result<Self, String> {
        if w.len() != Self::WORDS || w[0] != 1 {
            return Err("Unsupported settings format".into());
        }
        if w[1] > 57343
            || !(1..=2).contains(&w[2])
            || !(1..=255).contains(&w[3])
            || !(10..=500).contains(&w[5])
            || !(10..=500).contains(&w[9])
            || !(10..=2000).contains(&w[11])
            || [4, 6, 7, 8, 10].iter().any(|i| w[*i] > 1)
        {
            return Err("Settings exceed supported ranges".into());
        }
        let mut bindings = [Binding::default(); 5];
        for (i, b) in bindings.iter_mut().enumerate() {
            let p = 20 + i * 17;
            if w[p] > 1 || w[p + 1] > 1 {
                return Err("Invalid binding flags".into());
            }
            b.enabled = w[p] != 0;
            b.gesture = w[p + 1] != 0;
            for j in 0..5 {
                b.actions[j] = Action::decode(&w[p + 2 + j * 3..p + 5 + j * 3])?;
            }
        }
        Ok(Self {
            dpi: w[1] as u16,
            wheel_mode: w[2] as u8,
            threshold: w[3] as u8,
            vertical: w[4] != 0,
            vertical_gain: w[5] as f64 / 100.0,
            vertical_invert: w[6] != 0,
            smooth: w[7] != 0,
            horizontal: w[8] != 0,
            horizontal_gain: w[9] as f64 / 100.0,
            horizontal_invert: w[10] != 0,
            gesture_threshold: w[11] as i32,
            horizontal_actions: [Action::decode(&w[12..15])?, Action::decode(&w[15..18])?],
            bindings,
        })
    }
}
#[derive(Default)]
pub struct InputEngine {
    pub settings: Option<Settings>,
    pressed: [bool; 5],
    gesture_xy: [(i32, i32); 5],
    captured: [Binding; 5],
    horizontal_remainder: f64,
}
impl InputEngine {
    pub fn release_all(&mut self, mut emit: impl FnMut(Output)) {
        for i in 0..5 {
            if self.pressed[i]
                && !self.captured[i].gesture
                && let Some(event) = self.captured[i].actions[0].output(false)
            {
                emit(event);
            }
        }
        self.pressed = [false; 5];
        self.gesture_xy = [(0, 0); 5];
        self.horizontal_remainder = 0.0;
    }
    pub fn buttons(&mut self, current: &[u16], mut emit: impl FnMut(Output)) {
        let Some(s) = &self.settings else { return };
        for (i, cid) in CONTROL_IDS.iter().enumerate() {
            let down = current.contains(cid);
            if down == self.pressed[i] {
                continue;
            }
            if down {
                self.captured[i] = s.bindings[i];
                self.gesture_xy[i] = (0, 0);
                if self.captured[i].enabled
                    && !self.captured[i].gesture
                    && let Some(event) = self.captured[i].actions[0].output(true)
                {
                    emit(event);
                }
            } else if self.captured[i].enabled {
                let b = self.captured[i];
                if b.gesture {
                    let (x, y) = self.gesture_xy[i];
                    let index = gesture_direction(x, y, s.gesture_threshold);
                    if let Some(event) = b.actions[index].output(true) {
                        emit(event);
                    }
                    if let Some(event) = b.actions[index].output(false) {
                        emit(event);
                    }
                } else if let Some(event) = b.actions[0].output(false) {
                    emit(event);
                }
            }
            self.pressed[i] = down;
        }
    }
    pub fn motion(&mut self, x: i16, y: i16) {
        for i in 0..5 {
            if self.pressed[i] && self.captured[i].gesture {
                self.gesture_xy[i].0 = self.gesture_xy[i].0.saturating_add(x as i32);
                self.gesture_xy[i].1 = self.gesture_xy[i].1.saturating_add(y as i32);
            }
        }
    }
    pub fn wheel(
        &mut self,
        horizontal: bool,
        delta: i16,
        multiplier: f64,
        mut emit: impl FnMut(Output),
    ) {
        let Some(s) = &self.settings else { return };
        if delta == 0 {
            return;
        }
        let (gain, invert) = if horizontal {
            (s.horizontal_gain, s.horizontal_invert)
        } else {
            (s.vertical_gain, s.vertical_invert)
        };
        let scaled = delta as f64 / multiplier.max(1.0) * gain * if invert { -1.0 } else { 1.0 };
        if horizontal && s.horizontal_actions.iter().any(|a| a.kind != 0) {
            if self.horizontal_remainder.signum() != scaled.signum() {
                self.horizontal_remainder = 0.0;
            }
            self.horizontal_remainder += scaled;
            let count = self.horizontal_remainder.abs().floor().min(8.0) as usize;
            let a = s.horizontal_actions[usize::from(scaled < 0.0)];
            for _ in 0..count {
                if let Some(e) = a.output(true) {
                    emit(e);
                }
                if let Some(e) = a.output(false) {
                    emit(e);
                }
            }
            self.horizontal_remainder -= self.horizontal_remainder.signum() * count as f64;
            self.horizontal_remainder = self.horizontal_remainder.clamp(-8.0, 8.0);
        } else {
            emit(Output {
                kind: 3,
                code: 0,
                modifiers: 0,
                x: if horizontal { scaled * 40.0 } else { 0.0 },
                y: if horizontal { 0.0 } else { scaled * 40.0 },
            });
        }
    }
}
fn gesture_direction(x: i32, y: i32, threshold: i32) -> usize {
    let x = x as i64;
    let y = y as i64;
    if x.abs().max(y.abs()) < threshold as i64 {
        0
    } else if x.abs() > y.abs() {
        if x < 0 { 3 } else { 4 }
    } else if y < 0 {
        1
    } else {
        2
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn words() -> Vec<u32> {
        let mut w = vec![0; Settings::WORDS];
        w[0] = 1;
        w[2] = 2;
        w[3] = 16;
        w[5] = 100;
        w[9] = 100;
        w[11] = 80;
        w
    }

    fn put_action(w: &mut [u32], offset: usize, kind: u32, code: u32, modifiers: u32) {
        w[offset..offset + 3].copy_from_slice(&[kind, code, modifiers]);
    }

    fn button_action_offset(button: usize, action: usize) -> usize {
        20 + button * 17 + 2 + action * 3
    }

    #[test]
    fn validate_before_hardware_write() {
        let mut w = words();
        assert!(Settings::decode(&w).is_ok());
        w[5] = 0;
        assert!(Settings::decode(&w).is_err());
    }
    #[test]
    fn rejects_internal_or_ambiguous_output_actions() {
        let mut w = words();
        w[12] = 3;
        assert!(Settings::decode(&w).is_err());

        let mut w = words();
        w[12] = 5;
        w[13] = 1;
        assert!(Settings::decode(&w).is_err());

        let mut w = words();
        w[12] = 2;
        w[14] = 1;
        assert!(Settings::decode(&w).is_err());
    }
    #[test]
    fn gesture_dead_zone_and_directions() {
        assert_eq!(gesture_direction(5, 4, 80), 0);
        assert_eq!(gesture_direction(-90, 4, 80), 3);
        assert_eq!(gesture_direction(1, -100, 80), 1);
        assert_eq!(gesture_direction(i32::MIN, 0, 80), 3);
    }
    #[test]
    fn release_does_not_inject_second_shortcut() {
        assert!(
            Action {
                kind: 1,
                code: 8,
                modifiers: 1
            }
            .output(false)
            .is_none()
        );
    }
    #[test]
    fn mouse_button_cleanup_releases_pressed_button() {
        let mut w = words();
        w[20] = 1;
        w[22] = 2;
        w[23] = 2;
        let mut e = InputEngine {
            settings: Some(Settings::decode(&w).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];
        e.buttons(&[0x53], |v| out.push(v));
        e.release_all(|v| out.push(v));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].x, 0.0);
    }

    #[test]
    fn wheel_applies_vertical_and_horizontal_gain_and_inversion() {
        let mut w = words();
        w[4] = 1;
        w[5] = 250;
        w[8] = 1;
        w[9] = 150;
        let mut e = InputEngine {
            settings: Some(Settings::decode(&w).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];

        e.wheel(false, 4, 2.0, |v| out.push(v));
        e.settings.as_mut().unwrap().vertical_invert = true;
        e.wheel(false, 4, 2.0, |v| out.push(v));
        e.wheel(true, 4, 2.0, |v| out.push(v));
        e.settings.as_mut().unwrap().horizontal_invert = true;
        e.wheel(true, 4, 2.0, |v| out.push(v));

        assert_eq!(
            out.iter().map(|v| (v.x, v.y)).collect::<Vec<_>>(),
            vec![(0.0, 200.0), (0.0, -200.0), (120.0, 0.0), (-120.0, 0.0)]
        );
        assert!(out.iter().all(|v| v.kind == 3));
    }

    #[test]
    fn horizontal_actions_accumulate_fractional_motion_and_reset_on_reversal() {
        let mut w = words();
        w[8] = 1;
        w[9] = 10;
        put_action(&mut w, 12, 1, 10, 1);
        put_action(&mut w, 15, 1, 11, 1);
        let mut e = InputEngine {
            settings: Some(Settings::decode(&w).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];

        for _ in 0..3 {
            e.wheel(true, 5, 2.0, |v| out.push(v));
        }
        assert!(out.is_empty());
        e.wheel(true, 5, 2.0, |v| out.push(v));
        assert_eq!(out.iter().map(|v| v.code).collect::<Vec<_>>(), vec![10]);

        for _ in 0..3 {
            e.wheel(true, 5, 2.0, |v| out.push(v));
        }
        e.wheel(true, -10, 2.0, |v| out.push(v));
        assert_eq!(out.len(), 1);
        e.wheel(true, -10, 2.0, |v| out.push(v));
        assert_eq!(out.iter().map(|v| v.code).collect::<Vec<_>>(), vec![10, 11]);
    }

    #[test]
    fn horizontal_actions_emit_at_most_eight_per_wheel_report() {
        let mut w = words();
        w[8] = 1;
        put_action(&mut w, 12, 1, 10, 1);
        let mut e = InputEngine {
            settings: Some(Settings::decode(&w).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];

        e.wheel(true, 20, 1.0, |v| out.push(v));
        assert_eq!(out.len(), 8);
        e.wheel(true, 1, 1.0, |v| out.push(v));
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|v| v.kind == 1 && v.code == 10));
    }

    #[test]
    fn gesture_binding_emits_dead_zone_and_all_four_directions() {
        let mut w = words();
        w[20] = 1;
        w[21] = 1;
        for action in 0..5 {
            put_action(
                &mut w,
                button_action_offset(0, action),
                1,
                20 + action as u32,
                0,
            );
        }
        let settings = Settings::decode(&w).unwrap();

        for (x, y, code) in [
            (79, 0, 20),
            (0, -80, 21),
            (0, 80, 22),
            (-80, 0, 23),
            (80, 0, 24),
        ] {
            let mut e = InputEngine {
                settings: Some(settings.clone()),
                ..Default::default()
            };
            let mut out = vec![];
            e.buttons(&[CONTROL_IDS[0]], |v| out.push(v));
            e.motion(x, y);
            e.buttons(&[], |v| out.push(v));
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].code, code);
        }
    }

    #[test]
    fn concurrent_mouse_buttons_are_all_released_by_cleanup() {
        let mut w = words();
        w[20] = 1;
        put_action(&mut w, button_action_offset(0, 0), 2, 0, 0);
        w[37] = 1;
        put_action(&mut w, button_action_offset(1, 0), 2, 1, 0);
        let mut e = InputEngine {
            settings: Some(Settings::decode(&w).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];

        e.buttons(&CONTROL_IDS[..2], |v| out.push(v));
        e.release_all(|v| out.push(v));
        e.buttons(&[], |v| out.push(v));

        assert_eq!(
            out.iter().map(|v| (v.code, v.x)).collect::<Vec<_>>(),
            vec![(0, 1.0), (1, 1.0), (0, 0.0), (1, 0.0)]
        );
    }

    #[test]
    fn button_release_uses_binding_captured_when_press_began() {
        let mut old = words();
        old[20] = 1;
        put_action(&mut old, button_action_offset(0, 0), 2, 0, 0);
        let mut new = words();
        new[20] = 1;
        put_action(&mut new, button_action_offset(0, 0), 2, 1, 0);
        let mut e = InputEngine {
            settings: Some(Settings::decode(&old).unwrap()),
            ..Default::default()
        };
        let mut out = vec![];

        e.buttons(&[CONTROL_IDS[0]], |v| out.push(v));
        e.settings = Some(Settings::decode(&new).unwrap());
        e.buttons(&[], |v| out.push(v));
        e.buttons(&[CONTROL_IDS[0]], |v| out.push(v));

        assert_eq!(
            out.iter().map(|v| (v.code, v.x)).collect::<Vec<_>>(),
            vec![(0, 1.0), (0, 0.0), (1, 1.0)]
        );
    }
}
