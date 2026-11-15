//! The aircraft's electrical system and its failures.
//!
//! A 28-volt system as in a light aircraft (the airliner and the fighter are
//! treated alike, their generators standing in for the alternator): the
//! battery behind the master switch, the alternator charging it once the
//! engine turns fast enough, and what draws on them: the instruments and
//! radios, the lights, and the starter, which pulls the voltage down while it
//! cranks. Without power the starter, the radios, the electric gauges and the
//! lights are dead; the engine itself runs on its magnetos regardless.
//!
//! Failures are set from the pause menu (or at random): they act where the
//! real ones would, in plane.rs (engine, fuel leak), here (alternator,
//! battery) and in gauges.rs (vacuum pump, pitot and static ports).

/// What can fail.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Failures {
    pub engine: bool,
    pub alternator: bool,
    pub battery: bool,
    pub vacuum: bool,
    pub pitot: bool,
    pub static_port: bool,
    pub fuel_leak: bool,
}

impl Failures {
    /// How many there are, for going through them (`get`, `set`, `label`).
    pub const N: usize = 7;

    pub fn get(&self, i: usize) -> bool {
        [self.engine, self.alternator, self.battery, self.vacuum, self.pitot, self.static_port, self.fuel_leak][i]
    }

    pub fn set(&mut self, i: usize, v: bool) {
        *[&mut self.engine, &mut self.alternator, &mut self.battery, &mut self.vacuum, &mut self.pitot, &mut self.static_port, &mut self.fuel_leak][i] = v;
    }

    pub fn label(i: usize) -> &'static str {
        use crate::i18n::t;
        match i {
            0 => t!("Engine failure", "Panne moteur"),
            1 => t!("Alternator (generators)", "Alternateur (génératrices)"),
            2 => t!("Battery flat", "Batterie à plat"),
            3 => t!("Vacuum pump (attitude and heading gyros)", "Pompe à vide (horizon et conservateur de cap)"),
            4 => t!("Pitot tube blocked (airspeed)", "Tube Pitot bouché (anémomètre)"),
            5 => t!("Static port blocked (altimeter, VSI)", "Prise statique bouchée (altimètre, variomètre)"),
            _ => t!("Fuel leak", "Fuite de carburant"),
        }
    }

    pub fn any(&self) -> bool {
        (0..Self::N).any(|i| self.get(i))
    }
}

pub struct Electrics {
    pub master: bool,
    /// battery charge, 0..1
    pub charge: f64,
    /// bus voltage, and the battery's current (A; + charging, - discharging)
    pub volts: f64,
    pub amps: f64,
}

pub struct Systems {
    pub elec: Electrics,
    pub fail: Failures,
}

impl Default for Systems {
    fn default() -> Self {
        Systems { elec: Electrics { master: true, charge: 0.95, volts: 28.0, amps: 0.0 }, fail: Failures::default() }
    }
}

/// Battery capacity (A h) and what the alternator gives at most (A).
const BATTERY_AH: f64 = 24.0;
const ALTERNATOR_A: f64 = 60.0;

impl Systems {
    /// Power on the bus: the master on and enough volts to run things.
    pub fn powered(&self) -> bool {
        self.elec.master && self.elec.volts > 18.0
    }

    /// One step. `charging`: the engine turning the alternator fast enough;
    /// `starter`: the starter cranking; `lights`: the lights on.
    pub fn update(&mut self, dt: f64, charging: bool, starter: bool, lights: bool) {
        let e = &mut self.elec;
        if self.fail.battery {
            e.charge = 0.0;
        }
        if !e.master {
            // Everything off: the battery just sits.
            (e.volts, e.amps) = (0.0, 0.0);
            return;
        }
        let load = 5.0 + if lights { 4.0 } else { 0.0 } + if starter { 120.0 } else { 0.0 };
        let battery_v = |c: f64| if c > 0.0 { 23.0 + 2.0 * c } else { 0.0 };
        if charging && !self.fail.alternator {
            // The alternator carries the load and charges the battery, tapering as it fills.
            let spare = (ALTERNATOR_A - load).max(0.0);
            let into = if e.charge > 0.0 || !self.fail.battery { spare.min(30.0 * (1.0 - e.charge).max(0.05)) } else { 0.0 };
            let short = (load - ALTERNATOR_A).max(0.0);
            e.amps = into - short;
            e.volts = 28.0;
        } else {
            // On the battery: it sags under load and runs down.
            e.amps = -load;
            e.volts = (battery_v(e.charge) - 0.025 * load).max(0.0);
        }
        e.charge = (e.charge + e.amps * dt / 3600.0 / BATTERY_AH).clamp(0.0, 1.0);
        if self.fail.battery {
            e.charge = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_and_alternator() {
        let mut s = Systems::default();
        // Engine running: the alternator holds 28 V and charges.
        s.update(1.0, true, false, true);
        assert!(s.powered() && s.elec.volts == 28.0 && s.elec.amps > 0.0);
        // Alternator failed: on the battery, which runs down over an hour or two.
        s.fail.alternator = true;
        let c0 = s.elec.charge;
        for _ in 0..3600 {
            s.update(1.0, true, false, true);
        }
        assert!(s.elec.charge < c0 - 0.3 && s.elec.charge > 0.0, "{}", s.elec.charge);
        assert!(s.powered() && s.elec.amps < 0.0);
        // The starter pulls the volts down hard.
        let v = s.elec.volts;
        s.update(0.1, false, true, false);
        assert!(s.elec.volts < v - 2.0);
        // Run flat: no power.
        for _ in 0..20_000 {
            s.update(1.0, false, false, true);
        }
        assert!(!s.powered());
        // Master off: nothing on the bus, nothing drawn.
        let mut s = Systems::default();
        s.elec.master = false;
        s.update(60.0, false, true, true);
        assert!(!s.powered() && s.elec.charge == 0.95);
        // A flat battery stays flat even with the alternator.
        let mut s = Systems::default();
        s.fail.battery = true;
        s.update(1.0, false, false, false);
        assert!(!s.powered());
    }

    #[test]
    fn failures_by_number() {
        let mut f = Failures::default();
        assert!(!f.any());
        for i in 0..Failures::N {
            f.set(i, true);
            assert!(f.get(i));
        }
        assert!(f.engine && f.fuel_leak && f.any());
    }
}
