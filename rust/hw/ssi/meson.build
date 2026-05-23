
_libssi_rs = static_library(
  'ssi',
  files([
    'src/lib.rs',
    'src/spi.rs',
    'src/rspi.rs',
    'src/bindings.rs',
  ]),
  override_options: ['rust_std=2021', 'build.rust_std=2021'],
  rust_abi: 'rust',
  link_with: [
    _util_rs,
    _migration_rs,
    _bql_rs,
    _qom_rs,
    _hwcore_rs,
    _system_rs,
    _trace_rs,
  ],
  dependencies: [
    common_rs,
    glib_sys_rs,
    qemu_macros,
  ],
)

rust_devices_ss.add(when: 'CONFIG_X_SSI_RUST', if_true: [declare_dependency(
  link_whole: [_libssi_rs],
  variables: {'crate': 'ssi'},
)])
