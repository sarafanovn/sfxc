use crate::patch::Envelope;

pub fn length(e: &Envelope) -> f32 {
    e.attack + e.decay + e.sustain_time + e.release
}

pub fn amp(e: &Envelope, t: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    if t < e.attack {
        return t / e.attack;
    }
    let t = t - e.attack;
    if t < e.decay {
        return 1.0 + (e.sustain_level - 1.0) * t / e.decay;
    }
    let t = t - e.decay;
    if t < e.sustain_time {
        return e.sustain_level * (1.0 + e.punch * (1.0 - t / e.sustain_time));
    }
    let t = t - e.sustain_time;
    if t < e.release {
        return e.sustain_level * (1.0 - t / e.release);
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Envelope {
        Envelope { attack: 0.1, decay: 0.1, sustain_level: 0.5, sustain_time: 0.2, release: 0.2, punch: 0.0 }
    }

    #[test]
    fn segments() {
        let e = env();
        assert!((amp(&e, 0.05) - 0.5).abs() < 1e-6);
        assert!((amp(&e, 0.1) - 1.0).abs() < 1e-5);
        assert!((amp(&e, 0.15) - 0.75).abs() < 1e-6);
        assert!((amp(&e, 0.3) - 0.5).abs() < 1e-6);
        assert!((amp(&e, 0.5) - 0.25).abs() < 1e-6);
        assert_eq!(amp(&e, 0.61), 0.0);
        assert!((length(&e) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn punch_boosts_sustain_start() {
        // Sample just after segment boundaries: 0.2 - 0.1 - 0.1 is not exactly 0 in f32.
        let e = Envelope { punch: 0.5, ..env() };
        assert!((amp(&e, 0.2001) - 0.75).abs() < 1e-3);
        assert!((amp(&e, 0.3) - 0.625).abs() < 1e-3);
    }

    #[test]
    fn zero_times_do_not_divide_by_zero() {
        let e = Envelope { attack: 0.0, decay: 0.0, sustain_level: 1.0, sustain_time: 0.0, release: 0.0, punch: 0.0 };
        assert_eq!(amp(&e, 0.0), 0.0);
        assert!(amp(&e, 1.0).is_finite());
    }
}
