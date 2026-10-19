# linur — 开发路线图
> 本文档是英文文件 `docs/spec/02-roadmap.md` 的简体中文译本；两者若有冲突，以英文版为准。

状态：draft v0.3（第 4 轮修订；v0.2 是第 3 轮的重构）
与 `spec/00-requirements.md`（决策）和 `spec/01-rCore-vs-linux011.md`（实测差距）一起读。v1.0 的验收契约是 `spec/03-parity-checklist.md`。

## 1. 规划规则

1. **里程碑带 tag 和博客文章，步骤不带。** 里程碑会打 tag 并写成文章；它内部按天划分的步骤只是普通提交。
2. **步骤以天为单位。** 任何没法在几天内老实做完的事情，都要在开工前拆开。
3. **没有跑绿的运行，就不打 tag。** 里程碑以在干净克隆上 `just verify` 通过为终点；tag 注释里记录那次提交和那次运行。
4. **先有规格，再写代码。** 每个步骤的接口在实现之前就写下来；没写下契约的模块，之后没法交给同学负责。
5. **清单就是契约。** `spec/03` 在 M1 开始前就存在，并由每个里程碑更新；当它没有无法解释的缺口时，v1.0 就达成了。
6. **依赖只会往下走，绝不会往上走。** 替换掉某个 Tier-2 crate 的步骤，要在同一个 PR 里把它从 `Cargo.toml` 和 `DEPENDENCIES.md` 删掉。
7. **linur 自己的 tag 从 v0.1.0 开始**，跟着里程碑走；rCore 各章的 tag 留在 `Fux-i/rCore`。linur 的历史是全新的；M2.1 需要的五个找回的 lab 提交列在 `spec/01` 附录 A 中（全新历史的流程记录 `spec/99-lineage.md` 是私密的，不被跟踪）。
8. **架构相关代码只允许住在 cfg 门控的 arch 模块内。** 自 M2 起作为每个 PR 的审查项强制。

## 2. 规模图例

| 规模 | 含义 |
|---|---|
| S | ≈ 1–3 天 |
| M | ≈ 4–7 天 |
| L | ≥ 2 周；必须在开工前拆成 S/M 步骤 |

## 3. 里程碑

### M1 基座 / Foundation（M）→ `v0.1.0`

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M1.1 工程卫生 | 不用网络就能构建；分阶段镜像打包器，让 `TEST=1` 不再破坏 cargo 缓存；修好 `usertests` 的预期，并把那 16 个没登记的程序收进来；`.gitignore` 强制执行 D10（规格 01/02/03 与 `AGENTS.md` 被跟踪；需求文档、源流记录与博客文章不被跟踪）；`NOTICE` + SPDX；重写 README（名称、目标、rCore 源流、路线图链接）；初建 `deny.toml`（D7 三层规则）与 `DEPENDENCIES.md`（每 crate 一行：用途、层级、负责人、移除 issue） | S |
| M1.2 调试与可观测性 | panic 时带 file:line 和栈回溯；崩溃后仍能留下的启动日志；harness（测试装置）里能只跑一个失败测试的隔离；把 gdb 和 QEMU-monitor 的操作步骤写成文档；退役 `log`（这里自建的日志设施取而代之） | S |
| M1.3a 出错安全的拷贝与错误返回 | `copy_from_user` / `copy_to_user`；`ENOSYS` / `EINVAL` / `EFAULT`；消灭全部用户出错即内核 panic 的路径；负向测试 | S |
| M1.3b Linux ABI 与 errno | errno 化、六个参数、按架构划分的系统调用表（先做 riscv64）；`user_lib` 与全部 55 个用户程序完成迁移并全量回归 | M |
| M1.4 最小的 `just verify` 门禁 | 构建 + 启动 + 退出码断言 + 逐个测试的报告（riscv64：SiFive test device，把 code 移到高半部分并与 0x3333 做按位或；x86_64：`isa-debug-exit`，已验证存在于 QEMU 8.2.2）+ 串口日志和报告产物；CI workflow 本身（`git submodule update --init --recursive`、缓存、串口日志与报告制品）；一个必须失败的负向测试 | S |
| M1.5 x86_64 冒烟级别 bring-up（最小启动验证） | 自建 loader → 长模式 → 串口输出 → 在 CI 里干净退出；刻意**不做**用户态、不做设备、不做真正的中断处理；第一个 `hal` 接缝（seam），带两个只覆盖控制台和退出的实现；x86_64 的 compile-only CI 检查（`cargo check --target x86_64-unknown-none`，不要求启动）从这里开始，持续到 M5.4 完整移植落地 | M |

**门禁。** 按架构的能力矩阵 —— 每个里程碑门禁都写明自己的矩阵，矩阵逐步扩张，直到 M5 起两个架构都跑全套件。M1：riscv64 在干净克隆上连同全套件把 `just verify` 跑绿；x86_64 跑冒烟路径（loader → 长模式 → 串口 → 干净退出）加 compile-only 检查。负向测试按要求失败；ABI 负向测试证明 `EFAULT` / `ENOSYS` 的行为；文档已提交。
**不在 M1 中。** 功能开发。这个里程碑存在的意义，是让它之后的一切都可测试。

### M2 存储与自主模块 / Storage and owned modules（M）→ `v0.2.0`

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M2.1 重新落地丢失的功能 | `mmap` / `munmap` / `brk`、`dup2`、`sys_trace`、`spawn`、stride 和 `setprio` —— 移植到 M1 的 ABI 上；stride 的重新落地即最终调度器（保真度差异 F14）；实验课的程序成为回归套件的种子 | M |
| M2.2 带路径的 VFS | 真正的 inode/dentry/superblock 对象；路径遍历器；. 和 ..；cwd、`chdir` / `getcwd`；`mkdir` / `rmdir` / `unlink` / `link` / `rename` / `readdir`；与 Linux 完全一致的 `open` 标志；包括 `CLOEXEC` 在内的 fd 语义；`stat` / `fstat`、`lseek`、`link` / `unlink` / `nlink` 从 M2.1 移到这里 —— 在真正的 VFS 上一次写对 | M |
| M2.3 权限与挂载 | mode / uid / gid / umask、权限检查、`chmod` / `chown`；只支持一个文件系统的挂载表；`/dev` 骨架和设备节点 | S |
| M2.4 自建块驱动和缓冲区缓存 | 自建 virtio-blk（让 `virtio-drivers` 退役）；请求队列；带哈希队列、延迟写和 `sync` 的缓冲区缓存；DMA 页帧不再泄漏；退役 `spin`（随请求队列自建 spinlock） | M |
| M2.5 自建 ELF 加载器 | 让 `xmas-elf` 退役；shebang 脚本；`envp`；格式错误的镜像 → `ENOEXEC` | S |
| M2.6 C 写的物理分配器 | 新仓库 `linur-io/alloc`，**只**含物理页帧分配器：freestanding C（无标准库的 C）、极小的 C ABI、由 Rust 提供的内存原语、手写的 `build.rs` 加 `clang`、基于 assert 的宿主机测试、以子模块方式挂载；让栈式分配器退役；同一步骤里内核堆改为自建 Rust free-list 分配器（约百行，不进 C 仓库），让 `buddy_system_allocator` 退役 | M |
| M2.7 文件系统模块抽取 | `easy-fs` 移到 `linur-io/fs`，以子模块挂载在 `fs/` 并钉到某个 tag 上；宿主机测试移到 std 适配层 crate；pin bump（子模块指针更新）的流程走通一次 | S |

**门禁。** 实验课套件 + 文件系统压力测试 + 宿主机测试全绿；分配给 M2 的 Tier-2 crate 消失（`virtio-drivers`、`xmas-elf`、`buddy_system_allocator`、`spin`）；子模块 pin 流程端到端验证过。

### M3 交互 / Interaction — “我能用起来了”（M）→ `v0.3.0`

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M3.1 设备层与 tty | 主/次设备号分派；带队列和线路规程（ICANON、ECHO、ISIG）的 tty；先做串口后端 | M |
| M3.2 真正的信号 | 编号、掩码、pending 集合、用户态处理函数、处理函数栈帧和 restorer、`sigaction`、`SIGKILL` / `SIGSTOP` 语义；^C 触发 SIGINT | M |
| M3.3 用户态 | 自建 libc 长出 shell 和 coreutils（`ls`、`cat`、`cd`、`mkdir`、`rm`、`ln`、`ps`、`echo`）；`ps` 的进程信息来自一个新的文档化扩展系统调用（1000+ 段，与线程/同步调用同族），记录在 `spec/01` §4；现有的 55 个程序留作测试 | M |
| M3.4 VGA 控制台与键盘 | VGA/CGA 文本控制台，支持 ANSI 输出和滚屏；带修饰键的 PS/2 扫描码；控制台作为 tty 后端；ctrl-alt-del | M |
| M3.5 会话、进程组、时间 | 会话、进程组、控制终端；`utime` / `stime` 记账；`alarm`、`pause`、`nice`；墙上时钟：x86_64 用自建 CMOS RTC 驱动，riscv64 是架构范围的 DELTA F13 —— QEMU riscv `virt` 没有 RTC（已在 QEMU 8.2.2 上验证：`goldfish-rtc` 不可用），故其墙上时钟 = `stime` 设置的启动纪元 + jiffies | S |

**门禁。** 一段脚本化的交互会话（shell 读入一行、运行一个程序、^C 打断它）能在 CI 里无人值守地跑完；一次带 screendump（截屏）产物的人工走查。
**里程碑 F。** 这就是“我能用起来了，而且感觉像 0.11”的那个里程碑。v0.3.0 是一条自足的展示弧，也是负责人带宽受挤时的计划内降级点。

### M4 内存 / Memory（M）→ `v0.4.0`

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M4.1 写时复制（COW）的 `fork` | COW 页表；写缺页时拆开共享；可执行页共享 | M |
| M4.2 按需分页 | 惰性页帧分配、栈增长、文件支撑页（file-backed pages）的页缓存 | M |
| M4.3 真正的 `mmap` / `munmap` / `brk` | 建立在 M4.1 和 M4.2 之上的正确语义，包括共享页 | M |
| M4.4 OOM 策略与记账 | 明确的 out-of-memory（内存耗尽）行为；按进程的内存记账；压力测试 | S |

**门禁。** fork/COW 的隔离测试、`mmap` 测试、内存记账，以及一次刻意耗尽内存的压力运行。门禁制品同时记录 syscall 往返、上下文切换与缺页延迟 —— 只测量，不设优化目标，与 `spec/00` §4 的性能非目标一致。

### M5 双架构 / Two architectures（L，开工前先拆分）→ `v0.5.0`

**M5 开工前**须写好两份 ADR：x86_64 的块设备总线与中断控制器。推荐选型：v1.0 用 ATA PIO 走传统 0x1F0/0x3F7 端口（QEMU `pc` 机器兼容模式，硬编码端口，不做 PCI 枚举）加 8259 PIC；PCI 枚举、virtio-pci、AHCI 与 APIC 都是 v1.0 之后的候选（§6）。

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M5.1 配置 | `config/<board>.toml` → 生成的常量；两套板级配置都能构建并启动 | M |
| M5.2 workspace 拆分 | 内核 crate 拆成 workspace 里的多个 crate；内核包改名为 `linur`；为抽出来的 crate 补宿主机测试 | M |
| M5.3 真正落地的 HAL | 把 M1.5 的接缝推广到两套实现之上；通用代码里不再有 riscv 假设 | M |
| M5.4 x86_64 完整移植 | 启动链、中断（8259 PIC + PIT，由 ADR 钉定）、定时器、ring 3、按架构划分的系统调用表、分页、ATA PIO 块设备（传统 0x1F0/0x3F7 端口，由 ADR 钉定）、VGA 控制台 | L（拆成子步骤） |
| M5.5 riscv64 启动路径自主化 | 在 CI 里构建 RustSBI，或者自己写 M 模式垫片，让启动路径不再是一个随仓库携带的第三方二进制；退役 `sbi-rt`（手写 ecall 封装） | M |

**门禁。** 在 CI 里，同一套用户态套件在两个架构上都跑绿；riscv 启动路径由我们自己的流水线产出。

### M6 对齐 0.11 / Parity（M）→ `v1.0.0`

| 步骤 | 交付物 | 规模 |
|---|---|---|
| M6.1 系统调用补齐 | `ioctl`、`fcntl`、`access`、`utime`、`times`、`uname`、`sync`、`mknod`、`chroot`、第二个文件系统的 `mount`，以及 0.11 集合里剩下的部分 | M |
| M6.2 设备 | `/dev/null`、`/dev/port`、行为诚实的内存设备；带 `SIGPIPE` 的管道 | S |
| M6.3 行为审计 | 会话、信号、exec、定时器和错误语义逐项对着 0.11 走查；每处差异要么修掉，要么记录下来 | M |
| M6.4 清单走查 | `spec/03` 的每一行都变成 DONE、DELTA（带一个理由和一个保真度差异 id）或 DEFERRED（带一个理由） | S |
| M6.5 加固 | 规格保持最新；不存在没有移除 issue 的 Tier-2 依赖；演示脚本走完完整的 0.11 体验；tag 注释里附上那次运行；把 syscall 往返、上下文切换与缺页延迟记入门禁制品（只测量，不设优化目标） | S |

**门禁。** `spec/03` 在两个架构上都没有无法解释的缺口。这就是 v1.0 的定义。

## 4. 并行

计划里的步骤是顺序的，但下面这些步骤写入范围互不重叠，可以并发进行：M2.6 和 M2.7（各自的仓库）与 M2.4 和 M2.5 并行；M3.4（控制台）与 M3.5（会话）并行；M5.1（配置）与 M5.3（HAL）并行，也可与 M3 并行；M5.2（workspace 拆分）与 M4 并行；M5.3（HAL 审计）自 M3 起增量还债。里程碑 tag 顺序与门禁不变 —— 只放开步骤的执行顺序。门禁始终由负责人在合并后的结果上运行。

## 5. 模块抽取策略（子模块）

* 只有当 crate 有自己的使用方或自己的 CI 时才抽取。`easy-fs`（M2.7）和 C 分配器（M2.6）符合条件；其他都还不符合。
* 仓库命名为 `linur-io/<module>`；挂载点就是干净的模块路径（`fs/`、`alloc/`）。
* 横跨内核和模块的改动是两次提交加一次 pin bump —— 模块先落地。
* CI 必须执行 `git submodule update --init --recursive`；过期的 pin 是一种静默失败模式，所以门禁要断言钉住的提交与构建产物一致。
* agent 每个任务只在一个仓库里干活；绝不在同一个步骤里同时动子模块和它的父仓库。
* 记录一种路径覆盖（path override）的做法，这样不钉 pin 也能在树内编辑模块。

## 6. v1.0 之后的候选（不是承诺）

为真实硬件写自己的 M 模式垫片；ATA 和一条通往真实硬件的路径；PCI 枚举、virtio-pci、AHCI、APIC；兼容 MINIX v1 镜像并支持 0.11 的 a.out（L2/L3 复古阶段）；用 busybox 或 musl 做外部 oracle（判定测试结果的参照）；`/proc`；SMP；网络；第二个文件系统；用 `clone` 和 `futex` 取代自定义的线程系统调用。

## 7. 修订历史

| 版本 | 变更 |
|---|---|
| v0.1 | 15 个打了 tag 的阶段，S0–S14 |
| v0.2 | 重构为六个里程碑，步骤以天为单位；tag 和文章移到里程碑一级；新增调试/可观测性、x86_64 冒烟级别 bring-up、用户态、riscv 启动路径自主化和对齐清单契约；M1 合并了原先的 hygiene 和 ABI 两个阶段 |
| v0.3 | 第 4 轮修订：M1.3 拆分（出错安全的拷贝 / errno 加用户态迁移）；按架构的门禁能力矩阵；自 M1.5 起的 x86_64 compile-only CI；M2.1/M2.2 范围调整（文件类 syscall 在真 VFS 上一次落地）；Tier-2 退役归属（`log`→M1.2、`spin`→M2.4、`sbi-rt`→M5.5）；M5 开工前的 x86_64 设备 ADR（ATA PIO + 8259 PIC）；M5 步骤与 M3/M4 交错；M4/M6 门禁的延迟记录；v0.3.0 定为计划内降级点；规则 7 改指 `spec/01` 附录 A |
