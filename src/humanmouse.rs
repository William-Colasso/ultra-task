//! "Movimento humano do mouse": gera o trajeto do ponteiro entre dois pontos.
//! Função pura e com semente; quem toca os pontos (e o relógio) é o interpretador.
//!
//! O trajeto é uma curva de Bézier cúbica em arco, com ruído pequeno que some no fim,
//! percorrida com perfil de jerk mínimo (acelera no começo, freia no fim). A duração segue
//! a lei de Fitts. Às vezes o movimento passa um pouco do alvo e faz uma correção curta.

/// Intervalo entre dois pontos do trajeto, em ms (a 1× de velocidade).
pub const STEP_MS: u64 = 10;
/// Largura de alvo usada na lei de Fitts, em px.
const FITTS_W: f64 = 12.0;
/// Fitts: duração = A + B·log2(1 + d/W), em ms.
const FITTS_A: f64 = 80.0;
const FITTS_B: f64 = 110.0;
/// Desvio máximo do arco em relação à reta (fração da distância e teto em px).
const ARC_FRAC: f64 = 0.22;
const ARC_MAX: f64 = 180.0;
/// Ruído perpendicular máximo, em px.
const NOISE_PX: f64 = 1.5;
/// Passar do ponto: chance, fração da distância e teto em px.
const OVERSHOOT_CHANCE: f64 = 0.3;
const OVERSHOOT_FRAC: f64 = 0.06;
const OVERSHOOT_MAX: f64 = 25.0;
/// Abaixo desta distância não há arco nem passar do ponto.
const SHORT_PX: f64 = 40.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// O ponto `i` é enviado em `(i + 1) * STEP_MS` ms (a 1× de velocidade). O último é o alvo.
    pub points: Vec<(i32, i32)>,
}

impl Path {
    #[cfg(test)]
    pub fn duration_ms(&self) -> u64 {
        self.points.len() as u64 * STEP_MS
    }
}

/// Duração base (lei de Fitts) para uma distância em px.
pub fn fitts_ms(dist: f64) -> f64 {
    FITTS_A + FITTS_B * (1.0 + dist / FITTS_W).log2()
}

#[cfg(test)]
/// Maior distância de um ponto do trajeto até o segmento `from`–`to`, para uma distância `dist`.
pub fn envelope(dist: f64) -> f64 {
    (dist * ARC_FRAC).min(ARC_MAX) + NOISE_PX + OVERSHOOT_MAX + 2.0
}

/// Trajeto de `from` até `to`. Mesma semente → mesmo trajeto.
pub fn path(from: (i32, i32), to: (i32, i32), seed: u64) -> Path {
    let mut rng = fastrand::Rng::with_seed(seed);
    let (p0, p3) = (pt(from), pt(to));
    let dist = len(sub(p3, p0));
    if dist < 1.0 {
        return Path { points: vec![to] };
    }
    let overshoot = dist >= 150.0 && rng.f64() < OVERSHOOT_CHANCE;
    let mut pts = if overshoot {
        let dir = scale(sub(p3, p0), 1.0 / dist);
        let o = (dist * OVERSHOOT_FRAC).min(OVERSHOOT_MAX) * (0.5 + 0.5 * rng.f64());
        let side = (rng.f64() - 0.5) * o; // um pouco de lado também
        let past = add(add(p3, scale(dir, o)), scale(perp(dir), side));
        let mut a = segment(&mut rng, p0, past, len(sub(past, p0)), true);
        let b = segment(&mut rng, past, p3, len(sub(p3, past)), false);
        a.extend(b);
        a
    } else {
        segment(&mut rng, p0, p3, dist, true)
    };
    *pts.last_mut().unwrap() = to;
    Path { points: pts }
}

/// Um trecho de `a` até `b` (`b` exato no fim). `arc` liga a curva e o ruído.
fn segment(rng: &mut fastrand::Rng, a: (f64, f64), b: (f64, f64), dist: f64, arc: bool) -> Vec<(i32, i32)> {
    let dur = fitts_ms(dist) * (0.85 + 0.3 * rng.f64());
    let n = ((dur / STEP_MS as f64).round() as usize).max(1);
    let d = sub(b, a);
    let normal = if dist > 0.0 { perp(scale(d, 1.0 / dist)) } else { (0.0, 0.0) };
    let curvy = arc && dist >= SHORT_PX;
    let (o1, o2) = if curvy {
        // Arco de um lado só: nunca vira reta em distâncias longas.
        let m = (dist * ARC_FRAC).min(ARC_MAX);
        let sign = if rng.bool() { 1.0 } else { -1.0 };
        (sign * m * (0.25 + 0.75 * rng.f64()), sign * m * (0.25 + 0.75 * rng.f64()))
    } else {
        (0.0, 0.0)
    };
    let p1 = add(add(a, scale(d, 0.3)), scale(normal, o1));
    let p2 = add(add(a, scale(d, 0.7)), scale(normal, o2));
    // Ruído suave: soma de dois senos com fase/frequência aleatórias, nulo nas pontas.
    let (f1, f2, ph1, ph2) = (1.0 + rng.f64() * 2.0, 3.0 + rng.f64() * 3.0, rng.f64() * 6.3, rng.f64() * 6.3);
    let amp = if curvy { NOISE_PX } else { 0.0 };
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let s = 10.0 * t.powi(3) - 15.0 * t.powi(4) + 6.0 * t.powi(5); // jerk mínimo
            let p = bezier(a, p1, p2, b, s);
            let env = (std::f64::consts::PI * s).sin(); // 0 no começo e no fim
            let noise = amp * env * 0.5 * ((f1 * 6.3 * s + ph1).sin() + (f2 * 6.3 * s + ph2).sin());
            let p = add(p, scale(normal, noise));
            (p.0.round() as i32, p.1.round() as i32)
        })
        .collect()
}

fn pt(p: (i32, i32)) -> (f64, f64) {
    (p.0 as f64, p.1 as f64)
}
fn add(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}
fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}
fn scale(a: (f64, f64), k: f64) -> (f64, f64) {
    (a.0 * k, a.1 * k)
}
fn len(a: (f64, f64)) -> f64 {
    a.0.hypot(a.1)
}
fn perp(a: (f64, f64)) -> (f64, f64) {
    (-a.1, a.0)
}
fn bezier(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (w[0] * p0.0 + w[1] * p1.0 + w[2] * p2.0 + w[3] * p3.0, w[0] * p0.1 + w[1] * p1.1 + w[2] * p2.1 + w[3] * p3.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distância de `p` ao segmento `a`–`b`.
    fn dist_to_segment(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> f64 {
        let (p, a, b) = (pt(p), pt(a), pt(b));
        let ab = sub(b, a);
        let l2 = ab.0 * ab.0 + ab.1 * ab.1;
        let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * ab.0 + (p.1 - a.1) * ab.1) / l2).clamp(0.0, 1.0) };
        len(sub(p, add(a, scale(ab, t))))
    }

    const CASES: [((i32, i32), (i32, i32)); 6] = [((0, 0), (1000, 0)), ((500, 400), (20, 30)), ((10, 10), (13, 14)), ((0, 0), (0, 300)), ((-50, 80), (1900, 1000)), ((100, 100), (160, 100))];

    #[test]
    fn starts_near_origin_and_ends_exactly_on_target() {
        for seed in 0..200 {
            for (from, to) in CASES {
                let p = path(from, to, seed);
                assert_eq!(*p.points.last().unwrap(), to, "seed {seed} {from:?}->{to:?}");
                // O primeiro passo sai da posição atual: fica a poucos px dela.
                let first = p.points[0];
                let d = len(sub(pt(to), pt(from)));
                let step = len(sub(pt(first), pt(from)));
                assert!(step <= (d * 0.05).max(3.0), "seed {seed}: primeiro passo {step:.1} px para d={d:.0}");
            }
        }
        assert_eq!(path((7, 7), (7, 7), 1).points, [(7, 7)]);
    }

    #[test]
    fn stays_inside_envelope_around_the_line() {
        for seed in 0..300 {
            for (from, to) in CASES {
                let p = path(from, to, seed);
                let env = envelope(len(sub(pt(to), pt(from))));
                for &q in &p.points {
                    let d = dist_to_segment(q, from, to);
                    assert!(d <= env, "seed {seed} {from:?}->{to:?}: ponto {q:?} a {d:.1} px (limite {env:.1})");
                }
            }
        }
    }

    #[test]
    fn long_moves_are_not_straight() {
        for seed in 0..300 {
            for (from, to) in [((0, 0), (1000, 0)), ((0, 0), (0, 300)), ((900, 700), (100, 50))] {
                let p = path(from, to, seed);
                let max = p.points.iter().map(|&q| dist_to_segment(q, from, to)).fold(0.0, f64::max);
                assert!(max >= 5.0, "seed {seed} {from:?}->{to:?}: desvio máximo {max:.1} px");
            }
        }
    }

    #[test]
    fn duration_follows_distance() {
        for seed in 0..300 {
            for d in [5, 50, 200, 800, 2000] {
                let p = path((0, 0), (d, 0), seed);
                let dur = p.duration_ms() as f64;
                let base = fitts_ms(d as f64);
                // Trecho principal ±15%, mais a eventual correção (curta) e o arredondamento do passo.
                let max = base * 1.15 * 1.06 + fitts_ms(OVERSHOOT_MAX * 1.2) * 1.15 + 2.0 * STEP_MS as f64;
                assert!(dur >= base * 0.85 - STEP_MS as f64 && dur <= max, "seed {seed} d={d}: {dur} ms (base {base:.0})");
            }
            // Longe demora mais que perto.
            assert!(path((0, 0), (2000, 0), seed).duration_ms() > path((0, 0), (20, 0), seed).duration_ms());
        }
        assert!((200.0..=1500.0).contains(&fitts_ms(1000.0)));
    }

    #[test]
    fn seeds_change_the_path() {
        let a = path((0, 0), (800, 300), 1);
        assert_eq!(a, path((0, 0), (800, 300), 1), "mesma semente, mesmo trajeto");
        let distinct: std::collections::HashSet<Vec<(i32, i32)>> = (0..20).map(|s| path((0, 0), (800, 300), s).points).collect();
        assert!(distinct.len() >= 19, "só {} trajetos diferentes em 20 sementes", distinct.len());
    }

    #[test]
    fn some_paths_overshoot_and_correct() {
        let overshoots = (0..200).filter(|&s| path((0, 0), (1000, 0), s).points.iter().any(|q| q.0 > 1003)).count();
        assert!((20..=120).contains(&overshoots), "{overshoots} de 200 passaram do ponto");
    }

    #[test]
    fn accelerates_then_brakes() {
        let p = path((0, 0), (1200, 0), 3).points;
        let speed = |i: usize| len(sub(pt(p[i + 1]), pt(p[i])));
        let n = p.len();
        let mid = speed(n / 3).max(speed(n / 2));
        assert!(speed(0) < mid && speed(n - 2) < mid, "{:?}", (speed(0), mid, speed(n - 2)));
    }
}
