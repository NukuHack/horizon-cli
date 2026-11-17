fn playground_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::offical_native as m;
    use crate::game::glue_native as n;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: m::KEYS.iter().map(|k| k.map(String::from)).collect(),
    });
}
