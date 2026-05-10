use syn::Attribute;

pub enum TuonoMacros {
    Api,
    Handler,
    Middleware,
}
#[allow(dead_code)]
pub enum InternalTuonoMacros {
    Type,
}
#[allow(dead_code)]
enum AllTuonoMacros {
    TuonoMacros(TuonoMacros),
    InternalTuonoMacros(InternalTuonoMacros),
}
impl TuonoMacros {
    pub fn as_str(&self) -> &'static str {
        match self {
            TuonoMacros::Api => "api",
            TuonoMacros::Handler => "handler",
            TuonoMacros::Middleware => "middleware",
        }
    }
    #[allow(dead_code)]
    pub fn get_value_from_attr(&self, attrs: &[Attribute]) -> &str {
        let crate_name = env!("CARGO_PKG_NAME");
        attrs
            .iter()
            .find(|attr| {
                let path = attr.path();
                let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
                if segments.len() <= 2 {
                    return false;
                }
                segments[0] == crate_name && TuonoMacros::try_from(segments[1].clone()).is_ok()
            })
            .map_or("", |_| self.as_str())
    }
}

impl std::convert::TryFrom<String> for TuonoMacros {
    type Error = ();

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "api" => Ok(TuonoMacros::Api),
            "handler" => Ok(TuonoMacros::Handler),
            "middleware" => Ok(TuonoMacros::Middleware),
            _ => Err(()),
        }
    }
}
