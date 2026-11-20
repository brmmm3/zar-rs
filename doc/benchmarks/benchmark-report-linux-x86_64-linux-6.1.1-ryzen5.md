# linux-6.1.1 Precompiled Kernel Archive Benchmark

Run date: 2026-11-20T06:25:40+01:00
- Operating system: Linux-7.2.15-070215-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 5 2400G with Radeon Vega Graphics (x86_64, 8 logical cores)
- Benchmark storage (Source and archives): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1
- Benchmark storage (Extraction output): SSD / non-rotational (may be virtual); model: Samsung SSD 980 1TB; device: nvme1n1

![Host OS, CPU and storage information, archive size, compression time, full and three-file extraction times, and separate CPU and RAM charts](./benchmark-charts-linux-x86_64-linux-6.1.1-ryzen5.svg)

## Results

Source: [`linux-6.1.1.zar`](../../testdata/linux-6.1.1.zar)
Dataset: precompiled Linux kernel tree from a ZAR archive
Uncompressed source size: 1445.3 MiB
Regular files verified: 88,384
Symbolic links excluded from path/size verification: 56

| Format | Encryption | Cache | Archive size | Compression (min-max) | Extraction (min-max) | Extract 3 files (min-max) | Temp space (3 files) | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| zarlib | No | hot | 212.9 MiB | 5.45-6.05 s (239.0-265.0 MiB/s) | 11.12-12.74 s (113.5-130.0 MiB/s) | 0.471-0.766 s | 0 B | 84 % / 0.52 GiB | 65 % / 0.50 GiB |
| zarlib | Yes | hot | 213.0 MiB | 6.15-6.30 s (229.5-235.2 MiB/s) | 13.76-15.73 s (91.9-105.0 MiB/s) | 0.718-0.920 s | 0 B | 88 % / 0.60 GiB | 67 % / 0.55 GiB |
| zip | No | hot | 304.0 MiB | 44.66-49.24 s (29.4-32.4 MiB/s) | 22.31-32.01 s (45.1-64.8 MiB/s) | 0.033-0.045 s | 0 B | 77 % / 0.05 GiB | 76 % / 0.00 GiB |
| zip | Yes | hot | 306.3 MiB | 46.87-56.32 s (25.7-30.8 MiB/s) | 29.94-31.58 s (45.8-48.3 MiB/s) | 0.057-0.059 s | 0 B | 79 % / 0.05 GiB | 68 % / 0.00 GiB |
| 7z | No | hot | FAILED | - | - | - | - | - | - |
| 7z | Yes | hot | FAILED | - | - | - | - | - | - |
| tar | No | hot | 1512.4 MiB | 6.77-9.35 s (154.5-213.6 MiB/s) | 26.06-38.80 s (37.3-55.5 MiB/s) | 0.200-0.329 s | 0 B | 36 % / 0.00 GiB | 28 % / 0.00 GiB |
| tar | Yes | hot | 1512.4 MiB | 34.38-82.47 s (17.5-42.0 MiB/s) | 69.30-71.93 s (20.1-20.9 MiB/s) | 42.231-52.075 s | 0 B | 23 % / 2.43 GiB | 22 % / 0.64 GiB |
| tar.gz | No | hot | 262.1 MiB | 32.74-33.85 s (42.7-44.1 MiB/s) | 5.68-6.51 s (222.0-254.3 MiB/s) | 3.638-3.749 s | 0 B | 31 % / 0.01 GiB | 37 % / 0.01 GiB |
| tar.gz | Yes | hot | 262.1 MiB | 34.89-35.48 s (40.7-41.4 MiB/s) | 11.17-11.80 s (122.5-129.4 MiB/s) | 6.870-10.294 s | 0 B | 36 % / 0.02 GiB | 29 % / 0.06 GiB |
| zarlib | No | cold | 212.8 MiB | 5.68-5.68 s (254.7-254.7 MiB/s) | 9.17-9.17 s (157.6-157.6 MiB/s) | 0.398-0.398 s | 0 B | 81 % / 0.46 GiB | 50 % / 0.49 GiB |
| zarlib | Yes | cold | 213.2 MiB | 6.57-6.57 s (220.0-220.0 MiB/s) | 7.47-7.47 s (193.4-193.4 MiB/s) | 0.994-0.994 s | 0 B | 83 % / 0.49 GiB | 64 % / 0.52 GiB |
| zip | No | cold | 304.0 MiB | 56.50-56.50 s (25.6-25.6 MiB/s) | 26.54-26.54 s (54.4-54.4 MiB/s) | 0.040-0.040 s | 0 B | 40 % / 0.05 GiB | 75 % / 0.00 GiB |
| zip | Yes | cold | 306.3 MiB | 58.18-58.18 s (24.8-24.8 MiB/s) | 27.40-27.40 s (52.7-52.7 MiB/s) | 0.078-0.078 s | 0 B | 42 % / 0.05 GiB | 77 % / 0.00 GiB |
| 7z | No | cold | FAILED | - | - | - | - | - | - |
| 7z | Yes | cold | FAILED | - | - | - | - | - | - |
| tar | No | cold | 1512.4 MiB | 19.76-19.76 s (73.1-73.1 MiB/s) | 24.08-24.08 s (60.0-60.0 MiB/s) | 1.043-1.043 s | 0 B | 38 % / 0.00 GiB | 22 % / 0.00 GiB |
| tar | Yes | cold | 1512.4 MiB | 55.84-55.84 s (25.9-25.9 MiB/s) | 37.86-37.86 s (38.2-38.2 MiB/s) | 34.666-34.666 s | 0 B | 14 % / 0.28 GiB | 23 % / 0.40 GiB |
| tar.gz | No | cold | 262.1 MiB | 34.59-34.59 s (41.8-41.8 MiB/s) | 12.79-12.79 s (113.0-113.0 MiB/s) | 3.813-3.813 s | 0 B | 34 % / 0.01 GiB | 32 % / 0.01 GiB |
| tar.gz | Yes | cold | 262.1 MiB | 40.59-40.59 s (35.6-35.6 MiB/s) | 15.79-15.79 s (91.5-91.5 MiB/s) | 10.525-10.525 s | 0 B | 30 % / 0.02 GiB | 30 % / 0.03 GiB |

<details><summary>7z (encrypted=False, hot cache), run 1, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>
<details><summary>7z (encrypted=False, hot cache), run 2, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>
<details><summary>7z (encrypted=True, hot cache), run 1, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>
<details><summary>7z (encrypted=True, hot cache), run 2, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>
<details><summary>7z (encrypted=False, cold cache), run 1, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>
<details><summary>7z (encrypted=True, cold cache), run 1, full extraction: FAILED</summary>
<pre>7z extraction failed with exit code 2:
Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/nios2 : ../../../arch/nios2/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/openrisc : ../../../arch/openrisc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/powerpc : ../../../arch/powerpc/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/sh : ../../../arch/sh/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/scripts/dtc/include-prefixes/xtensa : ../../../arch/xtensa/boot/dts
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/bonding/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_locked_port.sh : ../../../net/forwarding/bridge_locked_port.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mdb.sh : ../../../net/forwarding/bridge_mdb.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_mld.sh : ../../../net/forwarding/bridge_mld.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_aware.sh : ../../../net/forwarding/bridge_vlan_aware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_mcast.sh : ../../../net/forwarding/bridge_vlan_mcast.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/bridge_vlan_unaware.sh : ../../../net/forwarding/bridge_vlan_unaware.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/local_termination.sh : ../../../net/forwarding/local_termination.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/no_forwarding.sh : ../../../net/forwarding/no_forwarding.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_actions.sh : ../../../net/forwarding/tc_actions.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/dsa/tc_common.sh : ../../../net/forwarding/tc_common.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/mlxsw/spectrum-2/rif_counter_scale.sh : ../spectrum/rif_counter_scale.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/lag_lib.sh : ../bonding/lag_lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/drivers/net/team/net_forwarding_lib.sh : ../../../net/forwarding/lib.sh
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copy_mc_64.S : ../../../../../arch/powerpc/lib/copy_mc_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_64.S : ../../../../../arch/powerpc/lib/copyuser_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/copyuser_power7.S : ../../../../../arch/powerpc/lib/copyuser_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/mem_64.S : ../../../../../arch/powerpc/lib/mem_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_64.S : ../../../../../arch/powerpc/lib/memcpy_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/copyloops/memcpy_power7.S : ../../../../../arch/powerpc/lib/memcpy_power7.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/mce/vas-api.h : ../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/nx-gzip/include/vas-api.h : ../../../../../../arch/powerpc/include/uapi/asm/vas-api.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-compat.h : ../.././../../../../arch/powerpc/include/asm/asm-compat.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/asm-const.h : ../../../../../../arch/powerpc/include/asm/asm-const.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/extable.h : ../../../../../../arch/powerpc/include/asm/extable.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/feature-fixups.h : ../../../../../../arch/powerpc/include/asm/feature-fixups.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/asm/ppc_asm.h : ../../../../../../arch/powerpc/include/asm/ppc_asm.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/primitives/word-at-a-time.h : ../../../../../arch/powerpc/include/asm/word-at-a-time.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_32.S : ../../../../../arch/powerpc/lib/memcmp_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/memcmp_64.S : ../../../../../arch/powerpc/lib/memcmp_64.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/stringloops/strlen_32.S : ../../../../../arch/powerpc/lib/strlen_32.S
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/asm/lppaca.h : ../../../../../../arch/powerpc/include/asm/lppaca.h
ERROR: Dangerous link path was ignored : linux-6.1.1/tools/testing/selftests/powerpc/vphn/vphn.c : ../../../../../arch/powerpc/platforms/pseries/vphn.c

Sub items Errors: 47

Archives with Errors: 1

Sub items Errors: 47
</pre>
</details>

"Extract 3 files" is the time to extract only `Makefile`, `COPYING`, `kernel/sched/core.c`
from the archive (for encrypted tar/tar.gz this includes decrypting the archive). "Temp space"
is the peak disk usage of the extraction output and the runner's temporary
directory during that extraction minus the size of the extracted files, sampled
every 5 ms (the maximum over all runs). Temporary files outside these
directories (e.g. in the system temp directory) are not captured.

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/extracted/` directory, then checked against the source tree
for matching regular-file paths and sizes. During its 4 extraction run(s), `zarlib` renamed 0 path(s) total (0 per run) due to case-folded collisions; generated suffixes are normalized only for comparison. The results
directory was emptied again after each check. Symlink entries are excluded
from verification. `zarlib` stores links but does not restore them by default;
ZIP and tar restore links, and 7z may skip unsafe relative links during
extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. CPU is system-wide;
RAM is the summed resident set size (RSS) of the archiver process and its
children, excluding the runner and unrelated processes. Shared memory may be
counted more than once. Samples are taken only while a benchmark process is
running; short operations may have no samples and are shown as N/A. The table
shows the average CPU % and RAM GiB across available samples for each phase.
Runtime values show
the minimum and maximum measured time for each phase. The charts plot full
extraction times and the times to extract the three selected files separately.
The time-series charts plot CPU and RAM on separate axes for compression and
extraction, with one line per archive format (solid = plain, dashed = encrypted).
The CPU model and operating system are collected from the host. Storage details
describe the volumes holding the source/archive files and extraction output.
The operating system may not expose physical media behind virtual or layered
storage; such cases are identified as unknown or non-rotational/possibly virtual.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.
- The installed 7-Zip does not support the zstd codec (only forks such as 7-Zip-zstd do); the `7z-zstd` benchmark is skipped.
- `zarlib`: `zarc` release build, default Zstandard compression level 3;
  encrypted runs use AES-256-GCM and the default header-and-data scope.
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links.
  Encrypted runs use the implementation's standard ZipCrypto mode
  (legacy/weak encryption).
- `7z-zstd`: 7z format with the Zstandard codec at level 3 (only run when the
  installed 7-Zip supports it, e.g. 7-Zip-zstd); otherwise like `7z`.
- `7z`: 7z format with LZMA2 at level 5. Encrypted runs use AES-256 and
  encrypted headers; symlinks are stored as links. The 7-Zip CLI accepts
  passwords only as a command-line switch; this runner generates a temporary
  random password and never saves it. During extraction, 7-Zip rejects
  relative symlink targets containing parent components; the runner
  accepts only the expected warnings for those source links and still
  verifies every regular file.
- `tar`: GNU tar without compression (a pure archiving baseline). The
  encrypted run times tar plus OpenSSL CMS AES-256-GCM encryption and is
  decrypted before extraction, like `tar.gz`.
- `tar.gz`: GNU tar with gzip's default compression level. Since tar.gz has no
  built-in encryption, the encrypted run times tar/gzip plus OpenSSL CMS
  AES-256-GCM encryption. It is decrypted before extraction. A temporary
  self-signed RSA certificate is used to wrap the CMS content key; the
  temporary private key is deleted when the benchmark exits.
- Compression and extraction are measured twice per hot-cache configuration
  and once per cold-cache configuration. The report shows the minimum and
  maximum runtime across those runs. If an archiver command fails in any
  repetition, its configuration is marked FAILED and its error is shown above;
  other runs continue. The runner waits 3 seconds between
  benchmark runs; this delay is not included in the measured operation times.
  Build time, source download/extraction, and post-extraction verification are
  also excluded from the measured operation times.

## Tool versions

- `zarlib`: `0.15.0`
- `cargo`: `cargo 1.99.1 (5f94df478 2026-08-27)`
- `tar`: `tar (GNU tar) 1.35`
- `openssl`: `OpenSSL 3.5.5 27 Jan 2026 (Library: OpenSSL 3.5.5 27 Jan 2026)`
- `7z`: `7-Zip 26.00 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-02-12`
- `gzip`: `gzip 1.14`
- `zip`: `This is Zip 3.0 (July 5th 2008), by Info-ZIP.`
- `unzip`: `UnZip 6.00 of 20 April 2009, by Debian. Original by Info-ZIP.`
- `zarc`: `zarc 0.15.0 (mimalloc)`

The generated archives and temporary key material are removed after the run.
The source ZAR archives remain in `testdata/`; extracted source trees remain in
`testresults/datasets/`.
