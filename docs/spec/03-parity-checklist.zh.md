# linur — Linux 0.11 对齐清单（v1.0 契约）
> 本文档是英文文件 `docs/spec/03-parity-checklist.md` 的简体中文译本；两者若有冲突，以英文版为准。

状态：draft v0.2。本文件在 M1 启动**之前**写成，并在每个里程碑门禁处更新。

## 使用方式

每一行最终都必须恰好是下列状态之一：

| 状态 | 含义 |
|---|---|
| `TODO` | 尚未开始；已指明所属里程碑 |
| `WIP M# → DONE M#` | 目前部分存在，正在补齐到规格要求；并指明把该行翻成 DONE 的里程碑 |
| `DONE` | 已实现、已由门禁测试、并留有证据 |
| `DELTA F#` | 有意与 0.11 不同；原因见 `spec/01` §5 中的保真度差异编号 |
| `DEFER` | 不在 v1.0 范围内，并给出了原因 |

**当两种架构上都不存在处于 TODO 或 WIP 的行，且每一行 DELTA 和 DEFER 都带有原因时，即达到 v1.0。** 负责人会在每个里程碑门禁处审查本文件。

## 1. 启动与机器

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 启动扇区阶段 | `bootsect.s`，恰好 512 B，加载 4 个扇区 | 自有的 x86_64 加载器，不受 512 字节约束 | TODO M5.4 |
| 机器设置 | `setup.s` 实模式：视频参数、内存大小、搬运内核镜像、GDT/IDT、CR0.PE | 长模式入口、内存探测、GDT/IDT/TSS | TODO M5.4（M1.5 的自有加载器已能进入长模式并输出串口；内存探测、IDT/TSS 等其余部分仍在 M5.4 —— `docs/x86_64-smoke.md`、`verify/x86-serial.log`） |
| 内存上限 | 16 MiB 物理内存上限 | 没有人为上限 | DELTA F4 |
| 任务上限 | `NR_TASKS` 64 | 没有人为上限 | DELTA F4 |
| 第二种架构 | 无（只有 i386） | x86_64 旗舰 + riscv64 移植，两者 CI 均绿 | TODO M5.4（自 M1.5 起 CI 运行 x86_64 冒烟路径——自有加载器 → 长模式 → 串口 → 干净退出——以及仅编译检查 `cargo check --target x86_64-unknown-none`；两种架构跑完整套件在 M5.4 —— `verify/report.json`） |
| 启动路径的归属 | 自包含的汇编，自举 | 启动路径中没有随仓库携带的第三方二进制 | TODO M5.5（x86_64 启动链自 M1.5 起已归我们；riscv64 仍由随仓库携带的 RustSBI 启动） |

## 2. 内存

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 页帧分配器 | `mem_map[]` 字节数组，反向扫描 | 自有的分配器，用 C 编写（D8） | TODO M2.6 |
| 地址转换 | 分段 + 分页，每个任务一个 LDT，64 MiB 窗口 | 按架构的页表式虚拟内存 | DELTA F3 |
| 写时复制（COW） | `copy_page_tables` 清除写位；`un_wp_page` | COW 的 `fork` | TODO M4.1 |
| 按需分页 | `do_no_page`，从可执行文件取页 | 按需分页 + 页缓存 | TODO M4.2 |
| 共享的可执行页 | `share_page` / `try_to_share` | 共享页 | TODO M4.3 |
| 堆增长 | `sys_brk` 移动一个末端指针，页由缺页调入 | `brk`，用户可见行为相同 | WIP M2.1 → DONE M4.3（M2.1 为即时（eager）`brk`；M4.3 加按需分页） |
| `mmap` | **无** | 提供（L4 形态的 ABI 与现代用户态都需要它） | DELTA F1（自 M2.1 起存在：匿名、固定地址、即时映射；M4.3 加 fd/offset 与共享页） |
| 内存耗尽 | `oom()` 杀死一个任务或 panic | 带记账的显式策略 | TODO M4.4 |

## 3. 进程

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 进程表 | `task[]` 数组，下标 0 是空闲任务 | 用户可见行为相同 | TODO M3.5（随会话/进程组齐备） |
| 状态 | RUNNING / INTERRUPTIBLE / UNINTERRUPTIBLE / ZOMBIE / STOPPED | 同样的五种状态 | WIP M1.3 → DONE M1.3 |
| `fork` | COW，子进程返回 0 | 相同，采用 COW | TODO M4.1 |
| `execve` | 仅 a.out ZMAGIC，支持 shebang，argv/envp | ELF、shebang、`envp` | DELTA F2 |
| `exit` / `waitpid` | 重新挂到 init 之下，选项 WNOHANG/WUNTRACED，cutime/cstime | 语义相同，默认阻塞 | WIP M1.3 → DONE M6.1 |
| 会话与进程组 | `pgrp`、`session`、`leader`、`setsid`、控制 tty | 相同 | TODO M3.5 |
| 记账 | utime / stime / cutime / cstime | 相同 | TODO M3.5 |
| 调度器 | 计数器 + 优先级补充，只在用户态抢占 | 公平的抢占式调度器；具体算法可以不同（F14） | DONE（stride，DELTA F14） |
| 线程 | **无** | 通过保留的 rCore 系统调用 1000–1032 提供，直到 `clone`/`futex` 落地 | DELTA F5 |

## 4. 信号

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 编号 | 1–22，`_NSIG` 32 | 编号相同 | TODO M3.2 |
| 投递 | 用户栈上的帧，带 restorer，发生在返回用户态的路径上 | 相同的机制 | TODO M3.2 |
| 屏蔽 | `blocked` 字；SIGKILL 与 SIGSTOP 不可屏蔽 | 相同 | TODO M3.2 |
| API | `signal`、`sigaction`、`sgetmask`、`ssetmask` | 名称与语义相同 | TODO M3.2 |
| 默认动作 | SIGCHLD 忽略，其余按信号位作为退出码终止 | 相同 | TODO M3.2 |
| 作业控制 | **无**（^Z 已定义但从未接入） | v1.0 不提供 | DEFER：0.11 里也没有 |

## 5. 文件系统

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| VFS 层 | 针对 inode 模式位的固定 switch 语句 | 真正的 inode/dentry/superblock 对象 | DONE（M2.2：superblock、dentry 与 inode 对象） |
| 路径 | 逐分量遍历，没有 MAX_PATH 检查 | 支持 dot、dotdot、cwd 的遍历器 | DONE（M2.2：`.`/`..` 走真实目录项；分量 27 字节、路径 4096 字节） |
| 名称长度 | 14 个字符 | 长文件名 | DELTA F11 |
| 磁盘格式 | MINIX v1，1 KiB 块，32 字节 inode，7+1+1 个区 | 沿 easy-fs 一脉的自有格式 | DELTA F12（v3：`mode`/`uid`/`gid`、26 个直接表项、设备号存在 `direct[0]` —— `docs/perm.md` §3） |
| 缓冲区缓存 | 哈希队列 + 空闲链表，预读，电梯 | 哈希队列，延迟写，`sync` | DONE（M2.4：16 个哈希桶里共 64 个缓存，按空闲链表淘汰并跳过被钉住的缓存，延迟写在淘汰或 `block_cache_sync_all()` 时落盘；`sync` 系统调用仍在 M6.1 —— `docs/block.md` §7） |
| 挂载 | 8 项的表，仅 MINIX，没有文件系统类型参数 | 挂载表，至少一种文件系统 | DONE（M2.3：`os/src/fs/mount.rs` 的挂载表只持有一种文件系统；`mount()`/`umount()` 仍在 M6.1 —— `docs/perm.md` §5） |
| 权限 | owner/group/other、`suser()`、`chmod`、`chown`、`umask` | 相同 | DONE（M2.3：磁盘上的 mode/uid/gid、`umask`、在 0.11 各调用点的 `permission()` 判定 —— `docs/perm.md` §2/§6） |
| fd 表 | `NR_OPEN` 20，全局文件表 64 | 没有人为上限 | DELTA F4（自 M2.2 起按描述符的 `CLOEXEC`） |
| 管道 | 管道 inode，一页，无读者时 `SIGPIPE` | 相同，带 `SIGPIPE` | TODO M6.2 |
| 设备节点 | `/dev/null`、`/dev/port` 为真实设备；mem 设备返回 EIO | 相同，行为诚实 | TODO M6.2（M2.3 给出 `/dev` 骨架和真正的设备节点 inode；`open` 在 M3.1 的设备表出现前返回 `ENXIO`，设备本身在 M6.2 —— `docs/perm.md` §5/§9） |
| 文件系统调用 | stat, fstat, lseek, dup, dup2, link, unlink, mkdir, rmdir, mknod, chmod, chown, utime, access, umask, sync, truncate | 全部提供 | WIP M2.2 → DONE M6.1（M2.2 新增 `stat`、`fstat`、`lseek`、`link`、`unlink`、`mkdir`、`rmdir`；M2.3 新增 `mknod`、`chmod`、`chown`、`access`、`umask` 与 mode/uid/gid 检查；余下 `utime` M3.5，`sync` M6.1） |
| `ioctl` / `fcntl` | tty ioctl 与终端线路设置；部分实现的 `fcntl` | 相同的子集 | TODO M6.1 |
| 符号链接、`/proc` | 无 | v1.0 不提供 | DEFER |

## 6. 设备与驱动

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 控制台 | VGA/CGA/MDA 文本，ANSI 解析器，80x25，滚动，蜂鸣器 | 带 ANSI 的 VGA 文本控制台 | TODO M3.4 |
| 键盘 | PS/2 扫描码集 1，修饰键，功能键，ctrl-alt-del | 相同的核心行为 | TODO M3.4 |
| 串口 | 8250，固定 2400 bps，两个端口 | 自有的 UART 驱动，可用的速率 | TODO M3.1 |
| 定时器 | 8253/8254，HZ 100，LATCH 11931 | 相同的 tick 速率；自有的 PIT/HPET 驱动 | TODO M5.4 |
| 中断控制器 | 8259 主片 + 从片，显式 EOI | 自有的 PIC 或 APIC 处理 | TODO M5.4 |
| 块设备 | 软盘 + ST-506/ATA，CHS，请求电梯 | 自有的带请求队列的驱动 | DONE（M2.4：自建的 legacy virtio-MMIO 驱动，8 个描述符的 split virtqueue 请求队列，DMA 放在静态区 —— `docs/block.md` §2–§4） |
| 软盘 | 完整的 FDC + DMA 驱动 | 不提供 | DEFER：博物馆里的硬件，对用户没有价值 |
| ramdisk | 可选，默认关闭 | v1.0 不提供 | DEFER |
| 设备层 | major/minor 分发表 | 相同的形态 | TODO M3.1 |

## 7. 时间

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| tick | HZ 100，`jiffies` | HZ 100，`jiffies` | WIP M1.3 → DONE M1.3 |
| 墙上时钟（x86_64） | 启动时读一次 RTC，之后会漂移 | 自建 CMOS RTC 驱动；时钟不漂移 | DELTA F13 |
| 墙上时钟（riscv64） | 启动时读一次 RTC，之后会漂移 | QEMU `virt` 上没有 RTC 硬件：`stime` 设定的启动纪元 + jiffies | DELTA F13 |
| `time` / `stime` / `times` | 自 epoch 起的秒数；jiffies；四个计数器 | 相同 | TODO M3.5（`times` 在 M6.1） |
| `alarm` / `pause` | 单个闹钟，可中断的 pause | 相同 | TODO M3.5 |
| `setitimer` / `gettimeofday` | 无 | v1.0 不提供 | DEFER |

## 8. 系统调用覆盖

0.11 定义了 **72 个条目，编号 0–71**，其中 **13 个是 `-ENOSYS` 桩**（17, 26, 31, 32, 35, 38, 44, 51, 52, 53, 56, 58, 62）。linur 对未实现的编号返回 `ENOSYS`，这与那些桩一致；已实现的 59 个调用在下表中跟踪。自 M1.3b 起 ABI 采用 Linux 形状：最多六个参数，失败返回 `-errno`（数值表见 `docs/errors.md` §6），分派表按架构划分。

| # | 系统调用 | linur 状态 |
|---|---|---|
| 0 | `setup` | DEFER：0.11 启动序列的启动期产物，不是用户可见的调用 |
| 1 | `exit` | DONE |
| 2 | `fork` | WIP M1.3 → DONE M4.1（COW） |
| 3 | `read` | DONE |
| 4 | `write` | DONE |
| 5 | `open` | DONE（M2.2：Linux 标志值、errno、按描述符 `CLOEXEC`；`mode` 自 M2.3 起使用，并被 umask 掩码） |
| 6 | `close` | DONE |
| 7 | `waitpid` | WIP M1.3 → DONE M6.1（M1.3 起阻塞；选项在 M6.1） |
| 8 | `creat` | DONE（M2.2：`open` 配 `O_CREAT`、`O_WRONLY`、`O_TRUNC` 的 libc 包装；与 Linux 一样没有独立编号） |
| 9 | `link` | DONE（M2.2：硬链接，磁盘上带 `nlink`） |
| 10 | `unlink` | DONE（M2.2；`rmdir` 即 `AT_REMOVEDIR` 0x200） |
| 11 | `execve` | WIP M1.3 → DONE M2.5（envp 与 shebang；errno 已在 M1.3 完成） |
| 12 | `chdir` | DONE（M2.2：每进程 cwd） |
| 13 | `time` | WIP M1.3 → DONE M3.5：目前返回单调毫秒；必须返回墙上时钟的秒数 |
| 14 | `mknod` | DONE（M2.3：`S_IFREG`/`S_IFCHR`/`S_IFBLK`/`S_IFIFO`；裸权限字或 `S_IFDIR` 为 `EINVAL`；umask 掩码权限位 —— `docs/perm.md` §7） |
| 15 | `chmod` | DONE（M2.3：只取权限位，类型保留） |
| 16 | `chown` | DONE（M2.3：`u32::MAX` 保持该半不变，与 0.11 的字面赋值不同 —— `spec/01` §4） |
| 18 | `stat` | DONE（M2.2：Linux riscv64 的 128 字节 `struct stat`） |
| 19 | `lseek` | DONE（M2.2：`SEEK_SET`/`SEEK_CUR`/`SEEK_END`） |
| 20 | `getpid` | DONE |
| 21 | `mount` | TODO M6.1 |
| 22 | `umount` | TODO M6.1 |
| 23 | `setuid` | TODO M6.1 |
| 24 | `getuid` | TODO M6.1 |
| 25 | `stime` | TODO M3.5 |
| 27 | `alarm` | TODO M3.5 |
| 28 | `fstat` | DONE（M2.2） |
| 29 | `pause` | TODO M3.5 |
| 30 | `utime` | TODO M3.5 |
| 33 | `access` | DONE（M2.3：请求的权限位；`F_OK` 只查存在；无法解析的路径返回 `EACCES`，与 0.11 一致） |
| 34 | `nice` | TODO M3.5 |
| 36 | `sync` | TODO M6.1（`block_cache_sync_all()` 就是它要调用的刷新 —— `docs/block.md` §7） |
| 37 | `kill` | WIP M1.3 → DONE M3.5：目前是五位掩码；必须接受真正的信号编号（M3.2）并支持进程组（M3.5） |
| 39 | `mkdir` | DONE（M2.2） |
| 40 | `rmdir` | DONE（M2.2：`unlink` 加 `AT_REMOVEDIR` 0x200） |
| 41 | `dup` | DONE |
| 42 | `pipe` | DONE |
| 43 | `times` | TODO M3.5 |
| 45 | `brk` | DONE（M2.1，即时） |
| 46 | `setgid` | TODO M6.1 |
| 47 | `getgid` | TODO M6.1 |
| 48 | `signal` | TODO M3.2 |
| 49 | `geteuid` | TODO M6.1 |
| 50 | `getegid` | TODO M6.1 |
| 54 | `ioctl` | TODO M6.1 |
| 55 | `fcntl` | TODO M6.1 |
| 57 | `setpgid` | TODO M3.5 |
| 59 | `uname` | TODO M6.1 |
| 60 | `umask` | DONE（M2.3：保存 `mask & 0o777` 并返回旧值；作用于 `O_CREAT` 的 `open`、`mkdir` 和 `mknod`） |
| 61 | `chroot` | TODO M6.1 |
| 63 | `dup2` | DONE（扩展 1043） |
| 64 | `getppid` | TODO M3.5 |
| 65 | `getpgrp` | TODO M3.5 |
| 66 | `setsid` | TODO M3.5 |
| 67 | `sigaction` | TODO M3.2 |
| 68 | `sgetmask` | TODO M3.2 |
| 69 | `ssetmask` | TODO M3.2 |
| 70 | `setreuid` | TODO M6.1 |
| 71 | `setregid` | TODO M6.1 |

linur 有而 0.11 没有的调用：`getcwd` 17、`rename` 38（0.11 的编号 38 只是一个 `-ENOSYS` 桩）与 `getdents64` 61；12 个线程与同步调用（1000–1032）、实验用调用 469，以及文档化扩展 1040–1043（`spawn`、`trace`、`set_priority`、`dup2` —— `dup2` 只因 riscv64 的 Linux 保留 `dup3`）。1040–1043 这一族沿用 1000+ 约定；它们都记录在 `spec/01` §4 中，属于有意为之的扩展，不构成对齐缺口。

## 9. 用户态与工具链

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| libc | 内核树之外的一个小型静态 libc | 我们自己的 libc，位于 `user/` | WIP M3.3 → DONE M3.3 |
| shell | 由 `init` exec 的 `/bin/sh`（rc） | 我们自己的 shell | TODO M3.3 |
| coreutils | 取决于发行版（ls, cat, cp, rm, mkdir, mount, ps…） | 一个小集合：ls、cat、cd、mkdir、rm、ln、ps、echo | TODO M3.3 |
| 可执行文件格式 | a.out ZMAGIC，仅静态 | ELF，仅静态 | DELTA F2 |
| 共享库 | 无 | v1.0 不提供 | DEFER |
| init | 先跑 `/etc/rc`，再跑 `/bin/sh` | 相同的形态 | TODO M3.3 |

## 10. 配置与测试

| 条目 | Linux 0.11 | linur 目标 | 状态 |
|---|---|---|---|
| 配置 | Makefile 加 `config.h` | `config/<board>.toml` → 生成的常量 | DELTA F8, TODO M5.1 |
| 测试套件 | 源码树里没有 | `make verify` + 宿主机测试 + 用户态套件 | DONE（M2.1 新增 13 项实验回归测试；M2.2 新增 8 个 VFS 程序；M2.3 新增 `perm_test`、`dev_test` 与 `mount_test`；M2.4 新增 `disk_stress_test`，共 76 项，另有四组宿主机测试用例（延迟写、缓存命中、超过 64 块的容量、块回收）—— `verify/report.txt`） |
| CI | 无 | 每个里程碑门禁，两种架构 | WIP M1.4 → DONE M5.4（自 M1.5 起按架构的能力矩阵：riscv64 完整套件、x86_64 冒烟路径（`docs/x86_64-smoke.md`）、x86_64 仅编译检查 —— `verify/report.json`；x86_64 的完整套件在 M5.4） |
| 对齐证据 | — | 本文件，在每个门禁处更新 | WIP → DONE M6.4 |

## 11. 差异登记表

`spec/01` §5 载有叙述和原因（F1–F14）。只在本清单中出现的额外差异现已在其中预留编号：F11 用长文件名代替 14 个字符、F12 沿 easy-fs 源流的自有磁盘格式、F13 不漂移的墙上时钟（x86_64 自建 CMOS RTC；riscv64 为架构范围偏离）、F14 未指明但公平的调度器。没有人为的任务/文件描述符上限仍由 F4 覆盖。

## 12. 签署确认

里程碑门禁会用状态和证据（测试名、产物或提交）更新受影响的那些行。只有当两种架构上每一行都是带原因的 DONE、DELTA 或 DEFER 时，v1.0 才会被打上标签。
