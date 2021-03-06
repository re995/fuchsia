// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    crate::colors::ColorScheme,
    anyhow::Error,
    carnelian::drawing::DisplayRotation,
    fidl_fuchsia_boot::{ArgumentsProxy, BoolPair},
    std::str::FromStr,
};

pub const MIN_FONT_SIZE: f32 = 15.0;
pub const MAX_FONT_SIZE: f32 = 150.0;

#[derive(Debug, Default)]
pub struct VirtualConsoleArgs {
    pub disable: bool,
    pub rounded_corners: bool,
    pub animation: bool,
    pub color_scheme: ColorScheme,
    pub display_rotation: DisplayRotation,
    pub font_size: f32,
    pub dpi: Vec<u32>,
}

impl VirtualConsoleArgs {
    pub async fn new_with_proxy(boot_args: ArgumentsProxy) -> Result<VirtualConsoleArgs, Error> {
        let mut bool_keys = [
            BoolPair { key: "virtcon.disable".to_string(), defaultval: false },
            BoolPair { key: "virtcon.rounded_corners".to_string(), defaultval: false },
            BoolPair { key: "virtcon.animation".to_string(), defaultval: false },
        ];
        let bool_key_refs: Vec<_> = bool_keys.iter_mut().collect();
        let mut disable = false;
        let mut rounded_corners = false;
        let mut animation = false;
        if let Ok(values) = boot_args.get_bools(&mut bool_key_refs.into_iter()).await {
            disable = values[0];
            rounded_corners = values[1];
            animation = values[2];
        }

        let string_keys = vec![
            "virtcon.colorscheme",
            "virtcon.display_rotation",
            "virtcon.font_size",
            "virtcon.dpi",
        ];
        let mut color_scheme = ColorScheme::default();
        let mut display_rotation = DisplayRotation::default();
        let mut font_size = 15.0;
        let mut dpi = vec![];
        if let Ok(values) = boot_args.get_strings(&mut string_keys.into_iter()).await {
            if let Some(value) = values[0].as_ref() {
                color_scheme = ColorScheme::from_str(value)?;
            }
            if let Some(value) = values[1].as_ref() {
                display_rotation = DisplayRotation::from_str(value)?;
            }
            if let Some(value) = values[2].as_ref() {
                font_size = value.parse::<f32>()?.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
            }
            if let Some(value) = values[3].as_ref() {
                let result: Result<Vec<_>, _> =
                    value.split(",").map(|x| x.parse::<u32>()).collect();
                dpi = result?;
            }
        }

        Ok(VirtualConsoleArgs {
            disable,
            rounded_corners,
            animation,
            color_scheme,
            display_rotation,
            font_size,
            dpi,
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::colors::LIGHT_COLOR_SCHEME,
        fidl_fuchsia_boot::{ArgumentsMarker, ArgumentsRequest},
        fuchsia_async as fasync,
        futures::TryStreamExt,
        std::collections::HashMap,
    };

    fn serve_bootargs(env: HashMap<String, String>) -> Result<ArgumentsProxy, Error> {
        let (proxy, mut stream) = fidl::endpoints::create_proxy_and_stream::<ArgumentsMarker>()?;

        fn get_bool_arg(env: &HashMap<String, String>, name: &String, default: bool) -> bool {
            let mut ret = default;
            if let Some(val) = env.get(name) {
                if val == "0" || val == "false" || val == "off" {
                    ret = false;
                } else {
                    ret = true;
                }
            }
            ret
        }

        fasync::Task::local(async move {
            while let Ok(Some(req)) = stream.try_next().await {
                match req {
                    ArgumentsRequest::GetStrings { keys, responder } => {
                        let vec: Vec<_> =
                            keys.into_iter().map(|key| env.get(&key).map(|s| &s[..])).collect();
                        responder.send(&mut vec.into_iter()).unwrap();
                    }
                    ArgumentsRequest::GetBools { keys, responder } => {
                        let mut iter = keys
                            .into_iter()
                            .map(|key| get_bool_arg(&env, &key.key, key.defaultval));
                        responder.send(&mut iter).unwrap();
                    }
                    _ => assert!(false),
                }
            }
        })
        .detach();

        Ok(proxy)
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_disable() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.disable", "true")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.disable, true);

        let vars: HashMap<String, String> = [("virtcon.disable", "false")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.disable, false);

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_rounded_corners() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.rounded_corners", "true")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.rounded_corners, true);

        let vars: HashMap<String, String> = [("virtcon.rounded_corners", "false")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.rounded_corners, false);

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_animation() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.animation", "true")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.animation, true);

        let vars: HashMap<String, String> = [("virtcon.animation", "false")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.animation, false);

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_color_scheme() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.colorscheme", "light")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.color_scheme, LIGHT_COLOR_SCHEME);

        let vars: HashMap<String, String> = HashMap::new();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.color_scheme, ColorScheme::default());

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_display_rotation() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.display_rotation", "90")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.display_rotation, DisplayRotation::Deg90);

        let vars: HashMap<String, String> = [("virtcon.display_rotation", "0")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.display_rotation, DisplayRotation::Deg0);

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_font_size() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.font_size", "32.0")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.font_size, 32.0);

        let vars: HashMap<String, String> = [("virtcon.font_size", "1000000.0")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.font_size, MAX_FONT_SIZE);

        Ok(())
    }

    #[fasync::run_singlethreaded(test)]
    async fn check_dpi() -> Result<(), Error> {
        let vars: HashMap<String, String> = [("virtcon.dpi", "160,320,480,640")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let proxy = serve_bootargs(vars)?;
        let args = VirtualConsoleArgs::new_with_proxy(proxy).await?;
        assert_eq!(args.dpi, vec![160, 320, 480, 640]);

        Ok(())
    }
}
