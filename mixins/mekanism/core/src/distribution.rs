// The even split an emitting network makes among its acceptors (`EmitUtils.sendToAcceptors`, `SplitInfo`, `Target`):
// each target that wants no more than its share gets what it wants, the rest share what is left, one extra unit
// going to as many as the remainder covers.

/// How much each target gets when `amount` is split among targets that can take at most `caps` each. The result is
/// in the order of `caps`. A port of `Target.sendToAcceptors`: `sendPossible`, `shiftNeeded` and `sendRemainingSplit`.
pub fn split(caps: &[u64], amount: u64) -> Vec<u64> {
    let n = caps.len();
    let mut given = vec![0u64; n];
    if n == 0 || amount == 0 {
        return given;
    }
    if n == 1 {
        given[0] = caps[0].min(amount);
        return given;
    }
    let mut info = Info::new(amount, n);
    // `accept(entry, resource, amount)`: what the target takes of `amount`, counted against what it has taken.
    let accept = |given: &mut Vec<u64>, i: usize, x: u64| {
        let taken = x.min(caps[i] - given[i]);
        given[i] += taken;
        taken
    };
    // `sendPossible`: a target that wants no more than its share takes it at once.
    let mut needed: Vec<(usize, u64)> = Vec::new();
    for i in 0..n {
        let wants = info.unsent.min(caps[i] - given[i]);
        if wants <= info.share() {
            info.send(wants, true);
            given[i] += wants;
        } else {
            needed.push((i, wants));
        }
    }
    // `shiftNeeded`, while the share keeps changing.
    while info.changed {
        info.changed = false;
        if needed.is_empty() || info.share() == 0 {
            continue;
        }
        let mut k = 0;
        while k < needed.len() {
            let (i, wants) = needed[k];
            if wants <= info.share() {
                let taken = accept(&mut given, i, wants);
                info.send(taken, true);
                needed.remove(k);
            } else {
                k += 1;
            }
        }
    }
    // `sendRemainingSplit`.
    if !needed.is_empty() && info.unsent > 0 {
        if needed.len() == 1 {
            let taken = accept(&mut given, needed[0].0, info.unsent);
            info.send(taken, false);
            return given;
        }
        let mut k = 0;
        while k < needed.len() {
            let remainder = info.remainder_share();
            if remainder == 0 {
                return given;
            }
            let (i, wants) = needed[k];
            if wants <= remainder {
                let taken = accept(&mut given, i, wants);
                info.send(taken, true);
                needed.remove(k);
            } else {
                let taken = accept(&mut given, i, remainder);
                info.send(taken, false);
                k += 1;
            }
        }
        let mut unsent = info.unsent;
        for &(i, _) in &needed {
            if unsent == 0 {
                break;
            }
            let taken = accept(&mut given, i, unsent);
            info.send(taken, false);
            unsent -= taken;
        }
    }
    given
}

/// `SplitInfo`.
struct Info {
    changed: bool,
    among: u64,
    unsent: u64,
    per: u64,
    remainder: u64,
}

impl Info {
    fn new(amount: u64, targets: usize) -> Self {
        let among = targets as u64;
        Self { changed: false, among, unsent: amount, per: amount / among, remainder: amount % among }
    }

    fn share(&self) -> u64 {
        self.per
    }

    fn remainder_share(&self) -> u64 {
        if self.among != 0 && self.remainder > 0 { self.per + 1 } else { self.per }
    }

    fn send(&mut self, amount: u64, decrement: bool) {
        self.unsent -= amount;
        if !decrement {
            if amount > self.per {
                self.remainder = self.remainder.saturating_sub(amount - self.per);
            }
            return;
        }
        self.among -= 1;
        if amount != self.per && self.among != 0 {
            let last = self.per;
            self.per = self.unsent / self.among;
            self.remainder = self.unsent % self.among;
            if !self.changed && self.per != last {
                self.changed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_even_split_when_everyone_wants_more() {
        assert_eq!(split(&[100, 100, 100], 90), vec![30, 30, 30]);
        assert_eq!(split(&[100, 100, 100], 100), vec![34, 33, 33]);
    }

    #[test]
    fn a_target_that_wants_less_leaves_more_for_the_others() {
        assert_eq!(split(&[10, 1000, 1000], 100), vec![10, 45, 45]);
        assert_eq!(split(&[0, 1000, 1000], 100), vec![0, 50, 50]);
    }

    #[test]
    fn nothing_is_lost_or_over_given() {
        for amount in [1, 7, 99, 1000] {
            let caps = [5, 40, 3, 1000, 17];
            let out = split(&caps, amount);
            assert_eq!(out.iter().sum::<u64>(), amount.min(caps.iter().sum()));
            assert!(out.iter().zip(caps).all(|(o, c)| *o <= c));
        }
        assert_eq!(split(&[8], 100), vec![8]);
        assert!(split(&[], 100).is_empty());
    }
}
