// The Dynarmic source patch is part of the wrapper crate's build script, and
// cargo never compiles build scripts with `cfg(test)`, so its tests would
// otherwise never run. Include the file here to exercise them.
#[path = "../src/cpu/dynarmic_wrapper/source_patch.rs"]
mod source_patch;
