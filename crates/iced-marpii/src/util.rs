///Gamma corrects a linear color, ignoring the alpha channel
pub fn gamma_correct(color: [f32; 4]) -> [f32; 4] {
    let (mut r, mut g, mut b) = (color[0], color[1], color[2]);
    r = r.powf(1.0 / 2.2);
    g = g.powf(1.0 / 2.2);
    b = b.powf(1.0 / 2.2);

    [r, g, b, color[3]]
}
