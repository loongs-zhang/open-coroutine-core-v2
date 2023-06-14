# open-coroutine

## 目录

- [诞生之因](#诞生之因)
- [三顾协程](#三顾协程)
- [语言选择](#语言选择)
- [核心特性](#核心特性)
- [架构设计](#架构设计)
- [底层抽象](#底层抽象)

## 诞生之因

2020年我入职W公司，由于内部系统不时出现线程池打满的情况，再加上TL读过[《Java线程池实现原理及其在美团业务中的实践》](https://tech.meituan.com/2020/04/02/java-pooling-pratice-in-meituan.html)
，我们决定构建自己的动态线程池，从结果来看，效果不错：

<div style="text-align: center;">
    <img src="img/begin.jpg" width="50%">
</div>

但是这没有从根本上解决问题。

众所周知，只要线程数超过CPU核心数就会带来额外的线程上下文切换开销，线程数越多，线程上下文切换开销越大。

对于CPU密集型任务，只需保证线程数等于CPU核心数(以下简称为`thread-per-core`)
，即可保证最优性能，而对于IO密集型任务，由于任务几乎必定阻塞住线程，线程上下文切换开销一般小于阻塞开销，但当线程数过大时，线程上下文切换开销就会大于阻塞开销了。

动态线程池的本质就是通过调整线程数，尽可能地让线程上下文切换开销小于阻塞开销。由于这个是人工保证的，那么必然保证不了。

<div style="text-align: center;">
    <img src="img/run.jpg" width="50%">
</div>

那么有没有一种技术能够在保证thread-per-core的前提下，执行IO密集型任务性能不输多线程呢？

答案是`NIO`，但仍存在一些限制或者不友好的地方：

1. NIO API使用起来较为复杂；
2. Thread.sleep()等阻塞调用会阻塞线程，相当于禁用所有阻塞调用，这对开发者不友好；
3. 在线程模型下，只有当前任务执行完了，才能执行下一个任务，无法实现任务间的公平调度；

PS：假设单线程，CPU时间片为1s，有100个任务，公平调度指每个任务都能公平地占用到10ms的时间片。

1还可以克服，2和3是硬伤，其实如果能够实现3，RPC框架们也不用搞太多线程，只要thread-per-core即可。

如何在能够保证thread-per-core、执行IO密集型任务性能不输多线程的前提下，使用还十分简单呢？

`协程`慢慢进入了我的视野。

## 三顾协程

一开始玩协程，出于学习成本的考虑，首先选择的是`kotlin`，但当我发现kotlin的协程需要更换API(
比如把Thread.sleep替换为kotlinx.coroutines.delay)才不会阻塞线程后，果断把方向调整为`golang`，大概2周后：

<div style="text-align: center;">
    <img src="img/good.jpeg" width="50%">
</div>

协程技术哪家强，编程语言找golang。

然而随着更深入的学习，我发现几个`goroutine`的不足：

1. `不是严格的thread-per-core`。goroutine运行时也是由线程池来支撑的，而这个线程池的最大线程为256，这个数字可比thread-per-core的线程数大得多；
2. `抢占调度会打断正在运行的系统调用`。如果这个系统调用需要很长时间才能完成，显然会被打断多次，整体性能反而降低；
3. `goroutine离极限性能有明显差距`。对比隔壁c/c++协程库，其性能甚至能到goroutine的1.5倍；

带着遗憾，我开始继续研究c/c++的协程库，发现它们要么是只做了`hook`(后面再详细解释)，要么只做了`任务窃取`
，还有一些库只提供最基础的`协程抽象`，而最令人失望的是：没有一个协程库实现了`抢占调度`。

没办法，看样子只能自己干了。

<div style="text-align: center;">
    <img src="img/just_do_it.jpg" width="100%">
</div>

## 语言选择

既然决定造轮子，那么需要选择开发轮子的语言。

之前研究c协程库时，有看到大佬已经尝试过用c写动态链接库、然后java通过jni去调这种方式，最终失败了，具体原因得深入JVM源码才能得知，对鄙人来说太高深，告辞，因此排除java/kotlin等JVM字节码语言。

显然，用golang再去实现一个goroutine，且不说其复杂程度完全不亚于深入JVM源码，而且即使真的做出来，也不可能有人愿意在生产环境使用，因此排除golang。

到目前为止还剩下c/c++/rust 3位选手。

从研究过的好几个用c写的协程库来看，c的表达力差了点，需要编写巨量代码。相较之下，c++表达力就强多了，但开发的效率还是低了些，主要体现在以下几个方面：

1. `需要不停地写cmake`，告诉系统怎么编译它，有些麻烦；
2. `依赖管理麻烦`。如果要用别人写的类库，把代码拉下来，放到自己项目里，然后需要耗费大量时间来通过编译。如果别人依赖的库没有其他依赖还好，一旦有其他依赖，那么它依赖的依赖，也得按照刚才说的步骤处理，这就十分麻烦了；
3. `内存安全`。c++很难写出没有内存泄漏/崩溃的代码。

<div style="text-align: center;">
    <img src="img/what_else_can_I_say.jpg" width="50%">
    <img src="img/rust.jpeg" width="100%">
</div>

## 核心特性

经过长时间的研究及实践，我认为一个完美的协程库应当同时具备以下5个特性：

1. `挂起/恢复`。协程可以在执行过程中挂起(即保存自己的上下文状态)，等某个异步操作返回结果后再恢复(即恢复自己的上下文状态)
   执行。挂起与恢复是协程最核心的点，它们的高效实现是协程能够实现异步操作和提高并发性能的关键所在；
2. `hook`。如果没有hook系统调用，并且未引入`抢占调度`机制，那么最终产出的协程库必定出现诸多限制，比如禁止使用sleep、禁止用阻塞socket读写数据等等；
3. `无栈协程`。线程在访问协程栈的数据时，由于线程栈所在的内存区域和协程栈所在的内存区域大概率不是连续的，所以很容易出现cache
   miss，而无栈协程由于直接使用线程栈，cache local显然更好；
4. `任务窃取`。在实际运行时，若不支持任务窃取，可能出现一核有难、多核围观的情况。支持任务窃取后，当前线程如果被某个协程阻塞住了，其他线程会把这个线程本地队列中的其他协程拿过来执行；
5. `抢占调度`。如果协程在运行过程中出现了死循环，可能导致所有调度协程的线程陷入死循环，最终可能导致服务不可用。引入抢占调度后，会自动挂起陷入死循环的协程，让其他协程执行。

<div style="text-align: center;">
    <img src="img/want_all.jpeg" width="100%">
</div>

PS：这里解释下hook技术，简单的说，就是函数调用的代理，比如调用sleep，没有hook的话会调用操作系统的sleep函数，hook之后会指向我们自己的代码，详细操作步骤可参考`《Linux/Unix系统编程手册》41章和42章`。

## 架构设计

<div style="text-align: center;">
    <img src="img/architecture.png" width="100%">
</div>

## 底层抽象

| 类库   | [context-rs](https://github.com/zonyitoo/context-rs)                                                                      | [corosensei](https://github.com/Amanieu/corosensei)                                | [genawaiter](https://github.com/whatisaphone/genawaiter)                  |
|------|---------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------|---------------------------------------------------------------------------|
| 类型   | 有栈协程                                                                                                                      | 有栈协程                                                                               | 无栈协程                                                                      |
| 完善程度 | 一般                                                                                                                        | 高                                                                                  | 高                                                                         |
| 优点   | 几乎支持所有操作系统及CPU架构，且可定制化程度高                                                                                                 | 提供了高性能&安全的抽象，汇编指令经过了深度优化，且支持[backtrace](https://github.com/rust-lang/backtrace-rs) | 目前最好的rust无栈协程基础库，支持[backtrace](https://github.com/rust-lang/backtrace-rs) |
| 缺点   | 不支持[backtrace](https://github.com/rust-lang/backtrace-rs)，且做支持的难度大；二开过程中容易踩坑，而且极难排查                                       | 不好做深度定制，后续无论是做减少协程切换次数的优化，还是做其他优化，难度都较大；受限于rust内联汇编的实现，只对主流系统及CPU架构做了支持            | 底层使用rust协程实现，无论是抢占调度还是hook都无法做到彻底                                         |
| 备注   | 其中[context](https://github.com/boostorg/context)的代码未更新，如果要写最好自己参考[context-rs](https://github.com/zonyitoo/context-rs)重新封装 | [作者](https://github.com/Amanieu)是rust语言社区的大佬                                       | rust async关键字的传染性是硬伤                                                      |

附上[协程切换方式性能对比](https://tboox.org/cn/2016/10/28/coroutine-context)
，如果是有栈协程，性能最好的底层是基于[context](https://github.com/boostorg/context)
做改造，直接抛弃对浮点数的支持，在x86_64下的linux，性能预计提升`125%~300%`。

暂时采用[corosensei](https://github.com/Amanieu/corosensei)，目前正在尝试自研无栈协程。

选好底层库，接着就是确定协程的状态了，下面是个人理解：

<div style="text-align: center;">
    <img src="img/state.png" width="50%">
</div>

## 时间轮

<div style="text-align: center;">
    <img src="img/time-wheel.png" width="50%">
</div>

## 任务窃取

## 调度器

## 抢占调度

## EventLoop

## JoinHandle

## 系统调用钩子

## 再次封装

## 极简属性宏

## 发布之后