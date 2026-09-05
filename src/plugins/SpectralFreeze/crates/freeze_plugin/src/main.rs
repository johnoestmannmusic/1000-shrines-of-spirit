use freeze_plugin::FreezePlugin;
use nih_plug::prelude::*;

fn main() {
    nih_export_standalone::<FreezePlugin>();
}
