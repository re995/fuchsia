# Set up and start the Fuchsia emulator (FEMU)

This guide provides instructions on how to set up and run the Fuchsia emulator (FEMU),
including networking and GPU support setup.

The steps are:

1. [Prerequisites](#prerequisites).
1. [Build Fuchisa for FEMU](#build-fuchsia-for-femu).
1. [Enable KVM (Optional)](#enable-kvm).
1. [Start FEMU](#start-femu).

## Prerequisites

Running FEMU requires that you've completed the following tasks:

 * [Checked out the Fuchsia source and set up the environment variables][get-fuchsia-source]
 * [Configured and built Fuchsia][build-fuchsia]

## Build Fuchsia for FEMU {#build-fuchsia-for-femu}

To run FEMU, you first need to build a Fuchsia system image that supports
the emulator environment. The example below uses `qemu-x64` for the board
and `workstation` for the product.

To build a FEMU Fuchsia image, do the following:

1. Set the Fuchsia build configuration:

   ```posix-terminal
   fx set workstation.qemu-x64 --release
   ```

2. Build Fuchsia:

   ```posix-terminal
   fx build
   ```

For more information on supported boards and products, see the
[Fuchsia emulator (FEMU)][femu-overview] overview page.

## Enable KVM (Optional) {#enable-kvm}

(**Linux only**) If KVM is supported on your machine, update permission
to enable KVM.

* {Linux}

  To enable KVM on your machine, do the following:

  Note: You only need to do this once per machine.

  1.  Add yourself to the `kvm` group on your machine:

      ```posix-terminal
      sudo usermod -a -G kvm ${USER}
      ```

  1.  Log out of all desktop sessions to your machine and then log in again.

  1.  To verify that KVM is configured correctly, run the following command:

      ```posix-terminal
      if [[ -r /dev/kvm ]] && grep '^flags' /proc/cpuinfo | grep -qE 'vmx|svm'; then echo 'KVM is working'; else echo 'KVM not working'; fi
      ```

      Verify that this command prints the following line:

      ```none {:.devsite-disable-click-to-copy}
      KVM is working
      ```

      If you see `KVM not working`, you may need to reboot your machine for
      the permission change in Step 2 to take effect.

* {macOS}

  No additional setup is required for macOS.

  Instead of KVM, the Fuchsia emulator on macOS uses the
  [Hypervisor framework][hypervisor-framework]{: .external}.

## Start FEMU {#start-femu}

Start the Fuchsia emulator on your machine.

* {Linux}

  The command below allows FEMU to access external networks:

  ```posix-terminal
  fx vdl start -N -u {{ '<var>' }}FUCHSIA_ROOT{{ '</var>' }}/scripts/start-unsecure-internet.sh
  ```

  Replace the following:

  * `FUCHSIA_ROOT`: The path to the Fuchsia checkout on your machine (for example, `~/fuchsia`).

  This command opens a new terminal window with the title "Fuchsia Emulator".
  After the Fuchsia emulator is launched successfully, the terminal starts a
  SSH console. You can run shell commands on this console, as you would on a
  Fuchsia device.

  However, if you want to run FEMU without access to external network,
  run the following command instead:

  ```posix-terminal
  fx vdl start -N
  ```

  The `-N` option enables the `fx` tool to discover this FEMU instance as a device
  on your machine.

* {macOS}

  To start FEMU on macOS, do the following:

  1. Start FEMU:

     ```posix-terminal
     fx vdl start
     ```

     If you launch FEMU for the first time on your macOS (including after a reboot),
     a window pops up asking if you want to allow the process `aemu` to run on your
     machine. Click **Allow**.

     This command prints an instruction on running `fx set-device`. Make a note of
     this command for the next step.

  2. Run the `fx set-device` command to specify the launched Fuchsia emulator SSH port:

     ```posix-terminal
     fx set-device 127.0.0.1:{{ '<var>' }}SSH_PORT{{ '</var>' }}
     ```

     Replace the following:

     * `SSH_PORT`: Use the value from the `fx vdl start` command's output in
     Step 1.

## Next steps

For the next steps, check out the following resources:

 *  To learn more about how FEMU works, see the
    [Fuchsia emulator (FEMU)][femu-overview] overview page.
 *  To learn more about Fuchsia device commands and Fuchsia workflows, see
    [Explore Fuchsia][explore-fuchsia].

## Appendices

This section provides additional FEMU options.

### Input options

By default FEMU uses multi-touch input. You can add the argument
`--pointing-device mouse` for mouse cursor input instead.

```posix-terminal
fx vdl start --pointing-device mouse
```

### Run FEMU without GUI support

If you don't need graphics or working under the remote workflow,
you can run FEMU in headless mode:

```posix-terminal
fx vdl start --headless
```

### Specify GPU used by FEMU

By default, FEMU launcher uses software rendering using
[SwiftShader][swiftshader]{: .external}. To force FEMU to use a specific
graphics emulation method, use the parameters `--host-gpu` or
`--software-gpu` to the `fx vdl start` command.

These are the valid commands and options:

<table><tbody>
  <tr>
   <th>GPU Emulation method</th>
   <th>Explanation</th>
   <th><code>fx vdl start</code> flag</th>
  </tr>
  <tr>
   <td>Hardware (host GPU)</td>
   <td>Uses the host machine’s GPU directly to perform GPU processing.</td>
   <td><code>fx vdl start --host-gpu</code></td>
  </tr>
  <tr>
   <td>Software (host CPU)</td>
   <td>Uses the host machine’s CPU to simulate GPU processing.</td>
   <td><code>fx vdl start --software-gpu</code></td>
  </tr>
</tbody></table>

### Supported hardware for graphics acceleration {#supported-hardware}

FEMU currently supports a limited set of GPUs on macOS and Linux for
hardware graphics acceleration. FEMU uses a software renderer fallback
for unsupported GPUs.

<table>
  <tbody>
    <tr>
      <th>Operating System</th>
      <th>GPU Manufacturer</th>
      <th>OS / Driver Version</th>
    </tr>
    <tr>
      <td>Linux</td>
      <td>Nvidia Quadro</td>
      <td>Nvidia Linux Drivers <a href="https://www.nvidia.com/download/driverResults.aspx/160175/en-us">440.100</a>+</td>
    </tr>
    <tr>
      <td>macOS</td>
      <td><a href="https://support.apple.com/en-us/HT204349#intelhd">Intel HD Graphics</a></td>
      <td>macOS version 10.15+</td>
    </tr>
    <tr>
      <td>macOS</td>
      <td>AMD Radeon Pro</td>
      <td>macOS version 10.15+</td>
    </tr>
  </tbody>
</table>

### Exit FEMU {#exit-femu}

To exit FEMU, run `dm poweroff` in the FEMU terminal.

### Configure IPv6 network {#configure-ipv6-network}

This section provides instructions on how to configure an IPv6 network
for FEMU on Linux machine using [TUN/TAP][tuntap]{: .external}.

* {Linux}

  Note: The `fx vdl` command automatically performs the instructions below.

  To enable networking in FEMU using
  [tap networking][tap-networking]{: .external}, do the following:

  1. Set up `tuntap`:

     ```posix-terminal
     sudo ip tuntap add dev qemu mode tap user $USER
     ```

  1. Enable the network for `qemu`:

     ```posix-terminal
     sudo ip link set qemu up
     ```

* {macOS}

  No additional IPv6 network setup is required for macOS.

  [User Networking (SLIRP)][slirp]{: .external} is the default network setup
  for FEMU on macOS – while this setup does not support Fuchsia device
  discovery, you can still use `fx` tools (for example,`fx ssh`) to
  interact with your FEMU instance.

<!-- Reference links -->

[get-fuchsia-source]: /docs/get-started/get_fuchsia_source.md
[build-fuchsia]: /docs/get-started/build_fuchsia.md
[femu-overview]: /docs/concepts/emulator/index.md
[hypervisor-framework]: https://developer.apple.com/documentation/hypervisor
[explore-fuchsia]: /docs/get-started/explore_fuchsia.md
[swiftshader]: https://swiftshader.googlesource.com/SwiftShader/
[tuntap]: https://en.wikipedia.org/wiki/TUN/TAP
[tap-networking]: https://wiki.qemu.org/Documentation/Networking#Tap
[slirp]: https://wiki.qemu.org/Documentation/Networking#User_Networking_.28SLIRP.29
