# linur 与 Linux 0.11 — 实测差距分析
> 本文档是英文文件 `docs/spec/01-rCore-vs-linux011.md` 的简体中文译本；两者若有冲突，以英文版为准。

状态：draft v0.2
基线：Linux 0.11 = kernel.org 上未经改动的 `linux-0.11.tar.gz`，sha256 `660997a6b1f03ee5aadce0ac806ffa653dedf1dea6d82c6731f9c9ddf0092673`，并在 elixir.bootlin.com 上做过交叉核对。linur = HEAD `4766af7`（“ch8-lab: deadlock detection”）处的 rCore 快照。下文每一条 linur 论断都引用了某个文件、某次 grep 或本次会话中的某次实测；每一条 0.11 论断都是从那个 tarball 里读出来的。从带注释的中文树（赵炯）里引用的数字一律不用 —— 那棵树按行数算大约大 20%。

## 0. 方法与证据

* 对 `os/`、`user/`、`easy-fs/`、`easy-fs-fuse/` 做只读源码勘察，引用精确到 file:line。
* 本次会话中完成的宿主机测试（见 §6）：完整构建、QEMU 启动，以及位于 `/tmp/linur-stage` 的分阶段镜像装置（harness），它能在不触碰构建缓存的前提下打包并启动任意用户态。
* 标为 **(unverified)**（未验证）的论断没有从一手来源得到确认。

## 1. 规模

| | Linux 0.11 | linur @4766af7 |
|---|---|---|
| 内核源码 | 约 100 个文件共 13,839 行（51 个 .c = 8,578；33 个 .h = 2,741；8 个 .s = 1,935；8 个 Makefile = 585） | `os/src` 中 3,992 行 |
| 用户态 | **不在源码树里** —— 单独的 MINIX 根镜像；`lib/`（392 行）是驻留在内核里的 mini-libc，供 `init` 使用 | `user/src` 中 3,715 行（一套自制的类 libc + 55 个测试程序） |
| 文件系统 | `fs/` 3,239 行（VFS + MINIX v1 + 缓冲区缓存） | `easy-fs` 1,240 行 + `easy-fs-fuse` 151 行 |
| 总计 | 13,839（仅内核） | 9,098 |
| 最大的子系统 | `fs/` 3,239；`kernel/` 2,106；`chr_drv/` 2,057 | `mm/`、`task/`、`syscall/`、`fs/` |

## 2. 形态概述

**0.11 完整但浅**：UNIX 需要的每个子系统它都有 —— 三段式启动链、能工作的驱动、tty 线路规程、VFS、带哈希队列和请求电梯（elevator）的缓冲区缓存、COW 加按需分页、带真正用户态处理函数的信号、会话与进程组 —— 每一样都只有几百行，而且没有哪一样比它需要的更深。

**linur 深但窄**：它有现代页表 VM（`SV39`，3 级）、线程与 PCB 分离并带 mutex/semaphore/condvar 和银行家算法死锁检测（这些设施 0.11 从来没有过），以及带间接块的磁盘文件系统 —— 而它坐在 SBI 之上，没有启动链，没有自己的驱动，没有路径，没有权限，没有 errno，没有 tty，没有记账，还习惯在 0.11 会返回错误的地方 panic。

这个项目要做的是：**把 UNIX 缺失的那一半建出来，并重新规定已有那一半底下的接口层。**

## 3. 子系统矩阵

| 子系统 | Linux 0.11（实测） | linur @4766af7（已验证） | 到 L1 的工作量 |
|---|---|---|---|
| **启动链** | `bootsect.s` 恰好 512 B → `setup.s` ≤ 4 个扇区（实模式、INT 15h 取内存大小、把镜像搬到 0x00000、临时 GDT/IDT、CR0.PE）→ 0x0000 处的 `head.s`（A20 检查、287/387 检测、pg_dir + 4 个页表 = 16 MiB 恒等映射，随后跳到 `main`）。构建时系统 ≤ 128 KiB。 | 从**随仓库携带的二进制** `bootloader/rustsbi-qemu.bin` 进入 S 模式（RustSBI 0.3.1，在别人的 WSL 机器上构建）；内核由 QEMU `-device loader` 加载到 0x80200000；`entry.asm` 设置一个栈并跳到 Rust。 | 自建 x86_64 启动链（multiboot2 或自己的 loader + 长模式设置）；riscv64 则在 CI 里构建 RustSBI，或者自己写一个 M 模式垫片。 |
| **架构抽象** | 不适用（只有 i386） | 一个硬编码的板级配置文件 `boards/qemu.rs`（CLOCK_FREQ 12.5 MHz、MEMORY_END 0x8800_0000、MMIO 列表），通过 `#[path]` 接进来；整个仓库里 `cfg(feature)` 命中 0 次 | HAL 接缝 + 按架构划分的 crate；真正的 trait 设计留到第二个移植目标（port #2） |
| **物理内存** | `mem_map[]` 字节数组覆盖 1–16 MiB；`get_free_page` **从后往前**扫描，并清零；`PAGING_PAGES` 为 3840 | **用 C 写的** refcount 字节图（`linur-io/alloc`，D8）覆盖 `[ekernel, 0x8800_0000)`，板级 128 MiB 的 RAM 窗口只占 32 KiB；**反向**扫描，每个页帧仍在 Rust 侧清零；内核堆与用户堆 = 自建 Rust free list | M2.6 已完成；COW 共享在 M4.1 补上 refcount 调用 |
| **虚拟内存** | 分段 + 分页：GDT/LDT，64 个任务 × 64 MiB 窗口，总计 4 GiB；内核恒等映射；`copy_page_tables` 清掉写位（**写时复制（COW）**），`un_wp_page`/`do_wp_page`/`write_verify`，`do_no_page`（**按需分页**），`share_page`/`try_to_share`（可执行页共享）。**从来没有 swap，也从来没有 mmap。** | `SV39`，3 级，硬编码；即时映射（每个页帧在建立映射时就分配）；fork = 完整复制，**没有 COW**；没有按需分页，没有栈增长（固定 8 KiB），没有共享内存，HEAD 里没有 `mmap`/`brk`（它们曾在 `7709ba3` 中存在，被 ch5 删掉） | COW fork、按需分页、`brk`/`mmap`、栈增长 |
| **进程模型** | `NR_TASKS` 为 64，`task[]` 数组，`find_empty_process`；TSS 硬件任务切换；5 种状态；`copy_process` + `copy_mem`；会话/进程组/`leader`/`kill_session`；控制终端；`utime`/`stime`/`cutime`/`cstime` | PCB（地址空间、fd、信号）+ TCB（上下文、内核栈）分离 —— 线程在这里存在，0.11 **没有**；fork 断言 `thread_count()==1`；FIFO 轮转，100 Hz tick；僵尸进程 + 重新挂到 INITPROC 上；没有会话/pgrp/tty；没有时间记账 | 不做 rlimit，但要来真的：会话、pgrp、tty 归属、记账 |
| **信号** | 定义了 1–22，`_NSIG` 为 32；位掩码 + `blocked`；SIGKILL/SIGSTOP 不可屏蔽；`signal`/`sigaction`；在返回用户态的路径上投递时会在用户栈上构造一个栈帧，基于 restorer（**没有 `sigreturn` 系统调用**）；默认动作 = `do_exit(1<<(signr-1))` | 一共 5 个位：SIGINT/SIGILL/SIGABRT/SIGFPE/SIGSEGV，**没有处理函数**，没有掩码，没有 pending 集合，没有 SIGKILL/SIGTERM/SIGCHLD/SIGSTOP；`kill(pid, 9)` 失败，因为第 9 位没有定义 | 处理函数 + 掩码 + 正确的编号；`sigsuspend` 一类阻塞 |
| **时间** | HZ 100，`LATCH` 11931 写入 PIT ch0；`jiffies`；启动时通过端口 0x70/0x71 加 `kernel_mktime` 读一次 RTC；`alarm()`、`times()`、`pause()`；**没有** `setitimer`/`gettimeofday` | 100 Hz 的 S 模式定时器；`get_time` = 从 `time` CSR 读到的自启动以来的单调毫秒数（**不是墙上时钟**）；`sleep(ms)` 基于定时器堆；不用 RTC，没有 alarm，没有 CPU 记账 | 墙上时钟 + alarm + 记账 |
| **系统调用 ABI** | `int 0x80`，eax = 编号，**最多 3 个参数**（ebx/ecx/edx），返回值放在 eax；72 个表项；越界 → **-1**；其中 13 个是 `-ENOSYS` 桩 | 分派形状相同，但 a0 = x17，**最多 3 个参数**，返回值放在 a0；28 个表项；**没有 errno**（用 `-1`/`-2` 当哨兵值）；未知 id 和未知的 `open` 标志会**让内核 panic** | 完整的 Linux 形态的一层：6 个参数、errno、按架构分的系统调用表、用 ENOSYS 代替 panic |
| **异常处理** | 用户写入之前先 `verify_area`；缺页 → `do_no_page`/`do_wp_page`；坏指针会被报告出来 | S 模式下发生的任何 trap 都会 **panic**；坏的用户指针会在拷贝辅助函数里 `unwrap()` 并**让内核 panic** | 出错安全的 `copy_from_user`/`copy_to_user`，返回 `EFAULT` —— 这是测试任何东西的前提 |
| **VFS** | 完全没有操作向量：`sys_read`/`sys_write` 按 inode 的模式位分发到 `rw_char`/`block_read`/`file_read`/`read_pipe`；`m_inode`/`file`/`super_block`/`buffer_head`；`NR_OPEN` 20、`NR_INODE` 32、`NR_FILE` 64、`NR_SUPER` 8、`NR_HASH` 307 | `trait File {readable, writable, read, write}` + `OSInode`/`Stdin`/`Stdout`/`Pipe`；偏移按每个打开文件计；只有一个全局根 inode；没有 dentry/inode/superblock 对象，没有挂载表 | 真正的 VFS 对象 + 路径解析器 + 挂载表 |
| **路径与文件名** | 直接从用户内存里逐分量遍历；**文件名 14 个字符**；不检查 MAX_PATH；`.`/`..` 是真实目录项；结尾的斜杠表示目录 | 在根目录里查一个名字；**没有 `/`，没有 `.`，没有 `..`，没有 cwd，没有挂载**；名字 28 字节（easy-fs） | 目录、`.`/`..`、cwd、`chdir`/`getcwd`、长文件名 |
| **磁盘文件系统** | 只有 MINIX v1：1024 字节块、32 字节 inode、16 字节目录项、7 个直接 + 1 个一级 + 1 个二级间接区、超级块魔数 0x137F；根设备写死在引导扇区里 | easy-fs：512 字节块、28 字节名字 + u32 inode 号、28 个直接 + indirect1 + indirect2，没有权限/uid/gid/时间戳/nlink | 定格式（第 2/3 轮）：延续 easy-fs 的源流，还是采用兼容 MINIX v1 的布局 |
| **缓冲区缓存** | 描述符放在 `&end`，数据从 `buffer_memory_end` 向下增长（跳过 0xA0000–0x100000 的空洞）；约数千个 1 KiB 缓冲区；**哈希队列（307）+ 空闲链表**；`bread`/`breada`（预读）/`bread_page`；`ll_rw_block` → `make_request` → 电梯调度（`NR_REQUEST` 32，优先处理读，每次请求 2 个扇区） | 16 个哈希桶里共 64 个写回缓存；未命中时淘汰该桶里最老且未被钉住的缓存（再按顺序找后续桶），脏缓存只在被淘汰或调用 `block_cache_sync_all()` 时落盘 | 供文件页使用的页缓存（M4.2） |
| **权限** | `permission(inode, mask)`，支持 owner/group/other 以及 `suser()`；`chmod`/`chown`/`access`/`umask`；uid/gid/euid/egid/suid/sgid 字段 | **完全没有** —— 磁盘上没有 uid/gid/mode，不做权限检查 | mode + 属主 + umask + chmod/chown |
| **管道** | pipe-inode，一页（4095 字节容量），读端/写端在 `i_wait` 上睡眠，没有读端时发 **SIGPIPE** | 32 字节环，阻塞式，所有写端关闭时 EOF，**不跟踪读端 → 没有 EPIPE/SIGPIPE**；向已被放弃的管道写入会永久阻塞 | POSIX 管道语义 |
| **控制台 / tty** | VGA/CGA/MDA 文本控制台 80x25，带完整的 ANSI 解析器和蜂鸣器；`tty_table[3]` = 控制台 + COM1 + COM2；`copy_to_cooked` 支持 ICANON/ECHO/ISIG；^C→SIGINT，^\\→SIGQUIT，**^Z 什么都不做**（`TSTPMASK` 定义了但从未接线）；1024 字节队列 | 只有 SBI `putchar`/`getchar`；stdin 的 `read` **断言 len == 1**（`read(0, buf, n>1)` 会 panic）；没有 tty，没有线路规程，没有回显，没有规范模式，没有作业控制 | tty 层 + 线路规程；控制台设备 |
| **键盘** | `keyboard.S`：IRQ1，扫描码集 1 的映射，shift/caps/ctrl/alt，F1–F12，LED，ctrl-alt-del；默认布局 KBD_FINNISH | 没有 | PS/2 驱动 |
| **中断** | 8259 主片 + 从片，按设备解除 IRQ 屏蔽，EOI；**中断驱动**的块 I/O，带请求队列 | 没有：定时器走 SBI，块 I/O 是**轮询**的，没有 PLIC，完全没有中断驱动的 I/O；S 模式的 trap 只要不是预期的就会 panic | PIC/APIC + IRQ 框架 + DMA |
| **块设备** | 软盘（FDC + DMA）和 ST-506/ATA（0x1f0–0x1f7，CHS，MAX_HD 2），可选的 ramdisk（默认关闭）；启动时读分区表 | 自建的 legacy virtio-MMIO 驱动位于 0x1000_1000：split virtqueue 请求队列，轮询，DMA 放在内核镜像的静态区 —— 不申请页帧，也就不会泄漏 | x86_64 ATA 驱动（M5.4） |
| **字符设备** | `/dev/null`，真正的 `/dev/port`（在 64 KiB 上做 inb/outb），`/dev/mem`/`/dev/kmem`/`/dev/ram` 全部返回 `-EIO`；按主设备号分派的表 | 没有设备节点，没有 `/dev`，没有 `ioctl`，没有 `mknod` | `/dev` + 主/次设备号分派 |
| **配置** | `Makefile` + `config.h`（`RAMDISK` 被注释掉；键盘布局 KBD_FINNISH 与 KBD_US/GR/FR） | `config.rs` 里硬编码的 `const`；没有 feature，没有 Kconfig，没有构建期配置 | `config/<board>.toml` → 生成的常量（D6） |
| **测试 / CI** | 源码树里没有（实验课另外接上 `testlab2.c` 之类） | 没有 CI，没有 `test` 目标，只有一个宿主机测试（`easy-fs-fuse::efs_test`，通过）；`make run TEST=1` 是交互式的，但**确实**会返回 QEMU 退出码 0 / 255（实测）；55 个应用里有 16 个从未被跑到；`usertests` 只是碰巧通过（一个缺失的应用被算作预期失败） | `make verify` 门禁 + 真正的矩阵 |
| **用户态 / libc** | 不带用户态；`lib/` 是 mini-libc；程序是 a.out ZMAGIC，`N_TXTOFF == 1024`，静态链接，没有共享库；`init` 先 exec `/etc/rc`，再 exec `/bin/sh` | 自制的 `user_lib`（没有 errno，没有 environ，没有 stdio 缓冲）；55 个测试程序，包括一个支持 `<`、`>`、`|` 且没有内建命令的 shell；应用是打包进 `fs.img` 的 RISC-V ELF | 自建 libc + shell + coreutils；ELF（已经是 ELF） |
| **ELF / exec** | 只有 a.out **ZMAGIC**，支持 `#!` 脚本，argv/envp 放在 MAX_ARG_PAGES 上 | 自建的 ELF64 解析器（`elf/`）位于 `MemorySet::from_elf` 之后：`PT_LOAD` 带 U/R/W/X，支持 `envp`、`#!` 脚本与按描述符的 `CLOEXEC`；没有 PIE/ASLR/`PT_INTERP` | 只剩 F2/F9：不支持 a.out 或 MINIX 镜像（L3）；PIE/`PT_INTERP` 不在范围内 |

## 4. 系统调用覆盖情况

* 0.11：**72** 个表项（0–71）；**13** 个是 `-ENOSYS` 桩；**59** 个已实现。
* linur：**50** 个表项 —— 33 个用 Linux/riscv64 编号（`getcwd` 17、`dup` 24、`mknod` 33、`mkdir` 34、`unlink` 35、`link` 37、`rename` 38、`access` 48、`chdir` 49、`chmod` 53、`chown` 54、`open` 56、`close` 57、`pipe` 59、`getdents64` 61、`lseek` 62、`read` 63、`write` 64、`stat` 79、`fstat` 80、`exit` 93、`sleep` 101、`yield` 124、`kill` 129、`umask` 166、`get_time` 169、`getpid` 172、`brk` 214、`munmap` 215、`fork` 220、`exec` 221、`mmap` 222、`waitpid` 260），一个实验用系统调用（469），12 个只用于教学的编号（1000–1032：线程、mutex、semaphore、condvar），以及四个文档化扩展（1040–1043：`spawn`、`trace`、`set_priority`、`dup2`）。1040–1043 这一族沿用 1000+ 扩展约定；`dup2` 之所以在这里，是因为 riscv64 的 Linux 只保留了 `dup3`。`creat` 不是表项：与 Linux 一样，它是 `open` 之上的 libc 包装。
* 与 0.11 已实现集合的重叠：**29** 个名字（`read`、`write`、`open`、`close`、`dup`、`pipe`、`exit`、`fork`、`execve`、`waitpid`、`getpid`、`kill`、`time`、`brk`、`dup2`，加上 M2.2 新增的 `creat`、`link`、`unlink`、`chdir`、`stat`、`lseek`、`fstat`、`mkdir`、`rmdir`，以及 M2.3 新增的 `mknod`、`chmod`、`chown`、`access`、`umask`）—— 其中**有九个在语义上分叉**（单调毫秒的 `time`、5 位的 `kill`、非阻塞的 `waitpid`、`open` 在缺失路径上对 `O_CREAT|O_DIRECTORY` 返回 `ENOTDIR` 而 Linux 返回 `EINVAL`、`brk` 返回新断点而 0.11 返回 0、`dup2` 拒绝超出当前表长度的 `newfd` 而 0.11 接受 `NR_OPEN` 以内的任意描述符、`stat` 填的是 Linux riscv64 的 128 字节布局而 0.11 填的是 i386 时代的布局、`creat` 只作为 `open` 之上的 libc 包装存在而 0.11 有独立的系统调用，以及 `chown` 把 `u32::MAX` 当作“该半保持不变”而 0.11 按字面赋值）。
* 0.11 已实现而 linur 缺失的（≈30 个）：`setup`、`mount`、`umount`、`setuid`/`getuid`/`geteuid`/`setreuid`、`setgid`/`getgid`/`getegid`/`setregid`、`stime`、`alarm`、`pause`、`utime`、`nice`、`sync`、`times`、`signal`、`sigaction`、`sgetmask`/`ssetmask`、`ioctl`、`fcntl`、`setpgid`、`uname`、`chroot`、`getppid`、`getpgrp`、`setsid`。
* linur 有而 0.11 没有的：`getcwd`、`rename`（0.11 声明了编号 38，但只是一个 `-ENOSYS` 桩）、`getdents64`，12 个线程/同步系统调用、`sleep`（毫秒）、`yield`、`enable_deadlock_detect`、`spawn`、`trace`、`set_priority`，外加一个计划中的文档化扩展，与线程调用同属 1000+ 段：为 `ps` 服务的进程信息调用（v1.0 没有 `/proc`，0.11 也没有 `ps` 可用的接口）。

**ABI 后果：** Linux 形态的接口（D1/L4）无法用一张表同时服务两个架构 —— x86_64 和 riscv64 的 Linux 系统调用编号不同（`open` 在 x86_64 上是 2，在 riscv64 上是 56；`fork` 是 57 对 220）。分派层必须按架构区分，同时共享语义内核。（已记录为待决问题。）

## 5. 有意为之的偏离（保真度差异）

这些属于主动选择，不能算缺口 —— 每一条都必须留在清单上，结论才站得住。

| # | 偏离 | 原因 |
|---|---|---|
| F1 | 用 Linux 形态的 ABI（6 个参数、errno、按架构分的表），而不是 0.11 的 3 参数/-1 ABI | 测试 oracle（判定测试结果的标准）：musl/busybox/LTP 形态的测试套件就变得可达了 |
| F2 | 用 ELF，而不是 a.out ZMAGIC | 2026 年工具链的现实 |
| F3 | 用现代分页（SV39 / x86_64 4 级页表），而不是 0.11 的分段+分页混合 | 0.11 的 64 MiB 窗口和 LDT 教不了更多东西，代价却很大 |
| F4 | 没有 16 MiB 内存上限，没有 64 任务数限制 | 人为限制算不上特性 |
| F5 | 从 rCore **保留**线程 + 同步原语 + 死锁检测 | 0.11 完全没有；这是 linur 唯一领先的地方 |
| F6 | 用 virtio-MMIO 而不是 ATA/软盘；自 M2.4 起是自己的驱动，ATA 仍是上真实硬件的路径（M5.4） | QEMU 里的软盘 DMA 是老古董；计划把 ATA 驱动作为上真实硬件的路径 |
| F7 | 保留 HZ 100；计划加墙上时钟 RTC | 0.11 只读一次 RTC，所以时钟会漂移；我们保留 tick，但把时钟修好 |
| F8 | 配置由 TOML 生成，而不是 `config.h` | D6 |
| F9 | 不带 0.11 原版用户态，也不带 MINIX 镜像 | L3 是更靠后的可选阶段 |
| F10 | 从 rCore 继承来的 GPLv3 | 快照带来的法律约束 |
| F11 | 用长文件名代替 0.11 的 14 字符名 | easy-fs 源流本就存 28 字节名；14 字符是人为限制 |
| F12 | 延续 easy-fs 源流的自有磁盘格式，而不是 MINIX v1 | L3（0.11 原版镜像）是靠后的可选阶段；easy-fs 已经存在 |
| F13 | 墙上时钟不漂移：x86_64 用自建 CMOS RTC 驱动；riscv64 为架构范围偏离——QEMU riscv `virt` 没有 RTC 设备（已在 QEMU 8.2.2 上实测），墙上时钟 = `stime` 设定的启动纪元 + jiffies | 0.11 只读一次 RTC，之后漂移；第二移植目标不应引入依赖环境的设备模型 |
| F14 | 调度器算法自由：公平的抢占式调度器；stride 重新落地已在 M2.1 完成，即具体算法 | 要点是用户可见的公平性，而不是复刻 0.11 的计数器 + 补充方案 |

## 6. 基线中实测到的缺陷（阶段 0 的输入）

1. **`make run TEST=1` 会破坏 cargo 构建缓存。** `user/Makefile:18` 执行 `cp usertests initproc`；cargo 把 `target/release/<bin>` 硬链接到 `target/release/deps/<bin>-<hash>`，所以这次 `cp` 会*穿过*硬链接写进缓存里的产物。之后每次构建都会把被破坏的二进制“恢复”回来。实测：`initproc` 变得与 `hello_world` 逐字节相同，`user_shell` 字符串数量为 **0**；用 `touch user/src/bin/initproc.rs` 加重新构建修好（`user_shell` 字符串 0 → 1）。因此重复跑 `TEST=1` 有测错程序的风险。
2. **`make build` 需要网络。** 它依赖伪目标 `env`，而后者每次构建都会跑 `cargo install cargo-binutils` —— 尽管 `rust-objcopy` 已经装好了。
3. **`usertests` 只是碰巧通过。** `race_adder_loop` 被引用但不存在；这个缺失的应用被算进 9 个预期失败里，所以总数仍然对得上。55 个程序里有 16 个从未被跑到，包括 ch8-lab 的全部三个死锁测试、`peterson`、`eisenberg`、两个协程测试、`early_exit*`、`count_lines`。
4. **`docs/` 没有被跟踪**（`git ls-files docs` 输出 0 个文件），而且 `/AGENTS.md` 被 `.gitignore` 隐藏，所以仓库自己的 agent 指南并没有进版本库。`README.md` 在工作区里已被删除。
5. **用户出错就让内核 panic**：坏的用户指针（拷贝辅助函数里的 `unwrap()`）、未知的 `open` 标志、未知的系统调用 id、任何 S 模式 trap。只要这一点还成立，就没有任何测试能断言 `EFAULT`/`ENOSYS`。
6. **`read(0, buf, n>1)` 会 panic**（`fs/stdio.rs` 里的 `assert_eq!(user_buf.len(), 1)`）。
7. **向已被放弃的管道写入会永久阻塞**（不跟踪读端，没有 EPIPE/SIGPIPE）。
8. **`fork`/`exec` 断言只有一个线程**，所以多线程进程无法 fork。
9. **Tier-0 依赖违规**：`sbi-rt`，外加随仓库携带的 RustSBI 二进制。钉在某个 git rev 上的 `virtio-drivers`、`xmas-elf`（ELF 解析）与 `buddy_system_allocator`（两个堆）曾在列，M2.4、M2.5 和 M2.6 分别用自己的块驱动、ELF 解析器，以及自建的帧分配器与堆替换了它们。
10. **实验特性已经从 HEAD 中消失**：`mmap`/`munmap`/`sbrk`（`7709ba3`）、`sys_trace`（`851e377`）、`spawn`（`c505a09`）、stride + `setprio`（`2a242b5`）、`link`/`unlink`/`fstat`（`c82eb2d`）。它们可以作为参考 diff 找回（约 1,500 行，含各自的测试程序，这些程序值得留作测试）。

## 7. linur 领先 0.11 的地方

带按区域权限的页表 VM；PCB/TCB 分离并有真正的线程；带等待队列的 mutex/semaphore/condvar 和银行家算法死锁检测；带块缓存与多级索引的磁盘文件系统；一个用户态 shell；Rust 工具链和一个 workspace；一套 0.11 从未有过的测试应用语料（55 个程序）。

## 8. 待决问题

带入第 2/3 轮：时间现实性与阶段数量；仓库名与历史策略；C 边界；阶段顺序；重新落地的实验特性是否立刻采用 Linux ABI；按架构分的系统调用表；“自己用得起来”这条标准对应的控制台与用户态目标；磁盘文件系统格式；riscv64 是继续用 3 参数风格的分派，还是先转向 Linux 的 6 参数约定。

## 附录 A —— 找回的 lab 提交

五个被回退的 lab 实现只存在于冻结的公开仓库 `Fux-i/rCore` 中（可从其 `main` 分支到达），从不在 linur 的历史里。路线图步骤 M2.1 需要它们作为参考 diff：

| lab | 提交 | 内容 |
|---|---|---|
| ch4-lab | `7709ba3` | `mmap` / `munmap` / `sbrk` |
| ch3-lab | `851e377` | `sys_trace` |
| ch5-lab1 | `c505a09` | `spawn` |
| ch5-lab2 | `2a242b5` | stride 调度 + `setprio` |
| ch6-lab | `c82eb2d` | `link` / `unlink` / `stat` |

M2.1 已把五个提交全部重新落地：语义移植到 M1 ABI，而不是 cherry-pick（`docs/reland.md`）。
