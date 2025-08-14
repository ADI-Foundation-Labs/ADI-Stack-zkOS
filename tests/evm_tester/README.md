# The EVM Tester


The `evm-tester` test crate runs tests for EVM compatibility of ZKsync OS.


## Usage

Each command assumes you are at the root of the `evm-tester` crate.

### Generic command

```bash
cargo run --release --bin evm-tester -- [-v] \
	[--path="${PATH}"]*
```

There are more rarely used options, which you may check out with `./target/release/evm-tester --help`.

To run all tests:

```bash
cargo run --bin evm-tester --features zksync_os_forward_system/no_print --release -- --spec_tests
```

## License

The EVM Tester is distributed under the terms of either

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.


## Official Links

- [Website](https://zksync.io/)
- [GitHub](https://github.com/matter-labs)
- [Twitter](https://twitter.com/zksync)
- [Twitter for Devs](https://twitter.com/ZKsyncDevs)
- [Discord](https://join.zksync.dev/)



## Disclaimer

ZKsync Era has been through extensive testing and audits, and although it is live, it is still in alpha state and
will undergo further audits and bug bounty programs. We would love to hear our community's thoughts and suggestions
about it!
It's important to note that forking it now could potentially lead to missing important
security updates, critical features, and performance improvements.
