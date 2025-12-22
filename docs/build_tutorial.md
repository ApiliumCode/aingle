# How to build AIngle SAF

*as of 28-10-2020*

## Steps

### 0. Build `aingle` and `ai`

You'll need two binaries on your PATH to develop SAFs: the actual AIngle (`aingle`) conductor binary, and the ai library which assists with assembling Wasms into a SAF file.

There are two ways you can approach that, via a [nix-shell](https://nixos.org/manual/nix/stable/#ch-installing-binary) which handles the majority for you, or via direct Rust installation to your computer. Instructions for both follow.

#### nix-shell

Install nix, **linux**
```bash
sh <(curl -L https://nixos.org/nix/install) --no-daemon
```
Install nix, **macOS**
```bash
sh <(curl -L https://nixos.org/nix/install) --darwin-use-unencrypted-nix-store-volume
```

Clone the aingle/aingle repo
```bash
git clone git@github.com:aingle/aingle.git
```

Enter the directory
```bash
cd aingle
```

Launch a nix-shell, based on aingle/aingle's nix-shell configuration
```bash
nix-shell
```

Install the `aingle` and `ai` binaries using the built-in installer
```bash
ai-install
```

Confirm that they are there by running `aingle -V` and `ai -V`, and that you see simple version number outputs.

#### native rust

Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Grab and install the binaries for `aingle` and `ai` from github, at the right version
```bash
cargo install aingle --git https://github.com/ApiliumCode/aingle.git --branch develop
cargo install ai --git https://github.com/ApiliumCode/aingle.git --branch develop
```

Confirm that they are there by running `aingle -V` and `ai -V`, and that you see simple version number outputs.

### 1. Write your Zomes

Each zome is a Rust crate. See [crates/test_utils/wasm/wasm_workspace/whoami](../crates/test_utils/wasm/wasm_workspace/whoami) and [crates/test_utils/wasm/foo](../crates/test_utils/wasm/wasm_workspace/foo) or any other folder in [crates/test_utils/wasm/wasm_workspace](../crates/test_utils/wasm/wasm_workspace) for examples.

### 2. Build your Zomes into Wasm

When you want to (re)build your zomes into Wasm, simply run

```bash
CARGO_TARGET_DIR=target cargo build --release --target wasm32-unknown-unknown
```

and they will be available in `target/wasm32-unknown-unknown/release/`

### 3. Assemble your Wasms into a SAF file

*Note: Soon, this process will be easier in that it will not require a `.saf.workdir`*

1. Create a `demo.saf.workdir` directory (replace "demo" with whatever you want)
2. Create a `demo.saf.workdir/saf.yaml` file which references the `*.wasm` files you built in the previous step. See the [saf.yaml](saf.yaml) file in this repo for an example.
  - Note: this is a bit hacky right now. Normally when using a saf.workdir, you would include the Wasms alongside the `saf.yaml` in the same directory. However, it is easier for the purposes of this tutorial to let the `saf.yaml` reference Wasms in a different directory. The workdir construct becomes more useful when you need to go back and forth between an already-built SAF and its constituent Wasms.
3. Run the following command to assemble your Wasms into a SAF file per your saf.yaml:

```bash
ai -c demo.saf.workdir
```

This will produce a `demo.saf` file as a sibling of the `demo.saf.workdir` directory.

### 4. Use the Conductor's admin interface to install your SAF

If you are using Tryorama to run tests against your SAF, you can jump over to the [tryorama (rsm branch) README](https://github.com/ApiliumCode/tryorama/tree/rsm) and follow the instructions there.

If you are running AIngle using your own setup, you'll have to have a deeper understanding of AIngle than is in scope for this tutorial. Roughly speaking, you'll need to:

- make sure `aingle` is running with a configuration that includes an admin interface websocket port
- send a properly encoded [`InstallApp`](https://github.com/ApiliumCode/aingle/blob/7db6c1e340dd0e741dcc9ffd51ffc832caa36449/crates/types/src/app.rs#L14-L23) command over the websocket
- be sure to `ActivateApp` and `AttachAppInterface` as well.
