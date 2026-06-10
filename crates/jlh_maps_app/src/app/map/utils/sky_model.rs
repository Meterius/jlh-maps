use bevy::color::Color;
use bevy::math::{Vec3, VectorSpace};

#[derive(Debug, Clone, Copy)]
pub struct LightingFactors {
    pub sun_color: Color,
    pub sun_intensity: f32,
    pub moon_color: Color,
    pub moon_intensity: f32,
    pub ambient_color: Color,
    pub ambient_intensity: f32,
}

pub fn lighting_from_sun_elevation(sun_elevation_deg: f32, moon_elevation_deg: f32) -> LightingFactors {
    let temp_k = sun_color_temperature(sun_elevation_deg);
    let sun_color = kelvin_to_rgb(temp_k);

    LightingFactors {
        sun_color,
        sun_intensity: sun_intensity(sun_elevation_deg),
        moon_color: moon_color(),
        moon_intensity: moon_intensity(moon_elevation_deg),
        ambient_color: ambient_color(sun_elevation_deg),
        ambient_intensity: ambient_intensity(sun_elevation_deg),
    }
}

fn moon_intensity(elevation_deg: f32) -> f32 {
    elevation_deg
        .to_radians()
        .sin()
        .max(0.0)
        .powf(0.6)
        .clamp(0.0, 1.0)
}

fn moon_color() -> Color {
    Color::srgb(0.62, 0.68, 1.0)
}

fn sun_color_temperature(elevation_deg: f32) -> f32 {
    if elevation_deg <= -6.0 {
        2000.0
    } else if elevation_deg <= 0.0 {
        2200.0
    } else if elevation_deg <= 8.0 {
        let t = elevation_deg / 8.0;
        2200.0_f32.lerp(3500.0, smoothstep(t))
    } else if elevation_deg <= 45.0 {
        let t = (elevation_deg - 8.0) / 37.0;
        3500.0_f32.lerp(5500.0, smoothstep(t))
    } else {
        let t = ((elevation_deg - 45.0) / 45.0).clamp(0.0, 1.0);
        5500.0_f32.lerp(6500.0, smoothstep(t))
    }
}

fn sun_intensity(elevation_deg: f32) -> f32 {
    if elevation_deg <= 0.0 {
        return 0.0;
    }

    elevation_deg.to_radians().sin().powf(0.4).clamp(0.0, 1.0)
}

fn ambient_color(elevation_deg: f32) -> Color {
    let night = Vec3::new(0.015, 0.025, 0.060);
    let twilight = Vec3::new(0.22, 0.20, 0.35);
    let sunrise = Vec3::new(0.95, 0.45, 0.25);
    let day = Vec3::new(0.55, 0.68, 1.00);

    let color_rgb = if elevation_deg <= -12.0 {
        night
    } else if elevation_deg <= -2.0 {
        let t = (elevation_deg + 12.0) / 10.0;
        night.lerp(twilight, smoothstep(t))
    } else if elevation_deg <= 6.0 {
        let t = (elevation_deg + 2.0) / 8.0;
        twilight.lerp(sunrise, smoothstep(t))
    } else if elevation_deg <= 25.0 {
        let t = (elevation_deg - 6.0) / 19.0;
        sunrise.lerp(day, smoothstep(t))
    } else {
        day
    };

    Color::srgb(color_rgb.x, color_rgb.y, color_rgb.z)
}

fn ambient_intensity(elevation_deg: f32) -> f32 {
    if elevation_deg <= -18.0 {
        0.02
    } else if elevation_deg <= 0.0 {
        let t = (elevation_deg + 18.0) / 18.0;
        0.02_f32.lerp(0.25, smoothstep(t))
    } else if elevation_deg <= 30.0 {
        let t = elevation_deg / 30.0;
        0.25_f32.lerp(0.85, smoothstep(t))
    } else {
        1.0
    }
}

fn kelvin_to_rgb(kelvin: f32) -> Color {
    let temp = kelvin / 100.0;

    let (mut r, mut g, mut b);

    if temp <= 66.0 {
        r = 255.0;
        g = 99.470_802_586_1 * temp.ln() - 161.119_568_166_1;

        b = if temp <= 19.0 {
            0.0
        } else {
            138.517_731_223_1 * (temp - 10.0).ln() - 305.044_792_730_7
        };
    } else {
        r = 329.698_727_446 * (temp - 60.0).powf(-0.133_204_759_2);
        g = 288.122_169_528_3 * (temp - 60.0).powf(-0.075_514_849_2);
        b = 255.0;
    }

    r = r.clamp(0.0, 255.0) / 255.0;
    g = g.clamp(0.0, 255.0) / 255.0;
    b = b.clamp(0.0, 255.0) / 255.0;

    Color::srgb(r, g, b)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn light_travel_direction_from_az_el_degrees(
    azimuth_degrees: f32,
    elevation_degrees: f32,
) -> Vec3 {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.clamp(-89.0, 89.0).to_radians();
    let horizontal = elevation.cos();

    Vec3::new(
        horizontal * azimuth.cos(),
        horizontal * azimuth.sin(),
        -elevation.sin(),
    )
        .normalize_or_zero()
}