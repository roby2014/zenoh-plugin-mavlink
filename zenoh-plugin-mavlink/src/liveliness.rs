use zenoh::key_expr::format::kedefine;

kedefine!(
    // Liveliness tokens key expressions
    pub ke_liveliness_all: "@/${zenoh_id:*}/@mavlink/${remaining:**}",
    pub ke_liveliness_plugin: "@/${zenoh_id:*}/@mavlink",
    pub(crate) ke_liveliness_sub: "@/${zenoh_id:*}/@mavlink/v2/in",
    pub(crate) ke_liveliness_pub: "@/${zenoh_id:*}/@mavlink/v2/out",
);