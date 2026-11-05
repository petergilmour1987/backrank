//! Match statistics: score, Elo difference with a 95% confidence interval, and the
//! likelihood of superiority (LOS), using the formulas from chessprogramming.org's
//! "Match Statistics".

use std::fmt;

/// Wins, draws and losses from one side's point of view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl Record {
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }

    /// Points per game, from 0 to 1. A record with no games scores 0.5.
    pub fn score(&self) -> f64 {
        if self.games() == 0 {
            return 0.5;
        }
        (f64::from(self.wins) + f64::from(self.draws) / 2.0) / f64::from(self.games())
    }

    /// The Elo difference the score implies. Infinite at 0% or 100%.
    pub fn elo(&self) -> f64 {
        elo_from_score(self.score())
    }

    /// The Elo difference, or for a whitewash (0% or 100%) the one-sided 95% bound on it.
    /// The bound comes from the "rule of three": after n straight losses, the true score
    /// is very likely below 3/n.
    pub fn elo_or_bound(&self) -> f64 {
        match self.score() {
            s if s <= 0.0 => elo_from_score(rule_of_three(self.games())),
            s if s >= 1.0 => elo_from_score(1.0 - rule_of_three(self.games())),
            _ => self.elo(),
        }
    }

    /// A 95% confidence interval for the Elo difference, from the normal approximation to
    /// the per-game score distribution. For a whitewash, the open side is infinite and the
    /// other comes from the rule of three.
    pub fn elo_interval(&self) -> (f64, f64) {
        let n = f64::from(self.games());
        if n == 0.0 {
            return (f64::NEG_INFINITY, f64::INFINITY);
        }
        let s = self.score();
        if s <= 0.0 || s >= 1.0 {
            let bound = self.elo_or_bound();
            return if s <= 0.0 {
                (f64::NEG_INFINITY, bound)
            } else {
                (bound, f64::INFINITY)
            };
        }
        let variance = (f64::from(self.wins) * (1.0 - s).powi(2)
            + f64::from(self.draws) * (0.5 - s).powi(2)
            + f64::from(self.losses) * s.powi(2))
            / n;
        let margin = 1.96 * (variance / n).sqrt();
        (elo_from_score(s - margin), elo_from_score(s + margin))
    }

    /// The likelihood of superiority: the probability that this side is the stronger,
    /// `0.5 * (1 + erf((W - L) / sqrt(2 (W + L))))`. Draws carry no information.
    pub fn los(&self) -> f64 {
        let decisive = f64::from(self.wins + self.losses);
        if decisive == 0.0 {
            return 0.5;
        }
        let lead = f64::from(self.wins) - f64::from(self.losses);
        0.5 * (1.0 + erf(lead / (2.0 * decisive).sqrt()))
    }
}

/// The 95% one-sided bound on a score after `games` straight losses: 3/n, capped at an even
/// score for tiny samples.
fn rule_of_three(games: u32) -> f64 {
    (3.0 / f64::from(games.max(1))).min(0.5)
}

/// `-400 log10(1/s - 1)`: the Elo difference that gives an expected score of `s`.
pub fn elo_from_score(s: f64) -> f64 {
    if s <= 0.0 {
        f64::NEG_INFINITY
    } else if s >= 1.0 {
        f64::INFINITY
    } else {
        -400.0 * (1.0 / s - 1.0).log10()
    }
}

/// The error function, by Abramowitz and Stegun's formula 7.1.26 (error below 1.5e-7).
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let y = 1.0 - poly * (-x * x).exp();
    y.copysign(x)
}

/// An Elo value for display: `+137`, `-52`, `+inf`.
pub struct Elo(pub f64);

impl fmt::Display for Elo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_infinite() {
            write!(f, "{}inf", if self.0 > 0.0 { '+' } else { '-' })
        } else {
            write!(f, "{:+.0}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(wins: u32, draws: u32, losses: u32) -> Record {
        Record {
            wins,
            draws,
            losses,
        }
    }

    #[test]
    fn elo_from_known_scores() {
        assert_eq!(elo_from_score(0.5), 0.0);
        assert!((elo_from_score(0.75) - 190.848).abs() < 0.01);
        assert!((elo_from_score(0.25) + 190.848).abs() < 0.01);
        assert!((elo_from_score(0.64) - 100.0).abs() < 0.2);
        assert_eq!(elo_from_score(1.0), f64::INFINITY);
        assert_eq!(elo_from_score(0.0), f64::NEG_INFINITY);
    }

    #[test]
    fn score_counts_draws_as_half() {
        let r = record(60, 20, 20);
        assert_eq!(r.games(), 100);
        assert!((r.score() - 0.7).abs() < 1e-12);
        assert_eq!(Record::default().score(), 0.5);
    }

    #[test]
    fn interval_contains_the_estimate_and_narrows_with_games() {
        let small = record(30, 10, 20);
        let big = record(300, 100, 200);
        let (lo, hi) = small.elo_interval();
        assert!(lo < small.elo() && small.elo() < hi);
        let (big_lo, big_hi) = big.elo_interval();
        assert!(big_hi - big_lo < (hi - lo) / 2.0);
        // About ±40 Elo for 240 games around an even score, as the plan says.
        let (lo, hi) = record(96, 48, 96).elo_interval();
        assert!(
            (hi - lo) / 2.0 > 35.0 && (hi - lo) / 2.0 < 45.0,
            "{lo} {hi}"
        );
    }

    #[test]
    fn whitewashes_have_rule_of_three_bounds() {
        let lost = record(0, 0, 240);
        assert_eq!(lost.elo(), f64::NEG_INFINITY);
        // 3/240 = 1.25%: -400 log10(79), about -759 Elo.
        let bound = lost.elo_or_bound();
        assert!((bound + 759.05).abs() < 0.01, "{bound}");
        assert_eq!(lost.elo_interval(), (f64::NEG_INFINITY, bound));
        let won = record(240, 0, 0);
        assert!((won.elo_or_bound() - 759.05).abs() < 0.01);
        assert_eq!(won.elo_interval().1, f64::INFINITY);
        // Tiny samples say no more than "not stronger".
        assert_eq!(record(0, 0, 2).elo_or_bound(), 0.0);
        assert_eq!(record(3, 1, 2).elo_or_bound(), record(3, 1, 2).elo());
    }

    #[test]
    fn likelihood_of_superiority() {
        // Equal wins and losses: 50%, to within the erf approximation's error.
        assert!((record(5, 10, 5).los() - 0.5).abs() < 1e-6);
        assert_eq!(record(0, 10, 0).los(), 0.5);
        // W - L = 10 over 100 decisive games is one standard deviation: about 84%.
        let los = record(55, 0, 45).los();
        assert!((los - 0.8413).abs() < 0.001, "{los}");
        assert!(record(10, 0, 50).los() < 0.001);
        assert!((erf(1.0) - 0.842_700_79).abs() < 1e-6);
    }

    #[test]
    fn elo_display() {
        assert_eq!(Elo(137.4).to_string(), "+137");
        assert_eq!(Elo(-52.6).to_string(), "-53");
        assert_eq!(Elo(f64::INFINITY).to_string(), "+inf");
        assert_eq!(Elo(f64::NEG_INFINITY).to_string(), "-inf");
    }
}
