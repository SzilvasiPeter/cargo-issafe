# A Hitchhiker's Guide to Unsafe Rust

"Don't Panic." - The ultimate advice for dealing with Unsafe Rust.

## History

For decades, systems programming in C and C++ has been like playing with loaded guns. Programmers have been shooting themselves in the foot since the 1970s via buffer overflows, use-after-free bugs, data races, and [null pointer](https://en.wikipedia.org/wiki/Null_pointer) dereferences. Speaking of which, Sir Tony Hoare introduced the null reference in 1965 and later famously apologized for it, calling it his "billion-dollar mistake" due to the billions of dollars in crashes and security vulnerabilities it subsequently spawned.

Linux, the open-source software that runs everything, is written in C and maintained by some of the most brilliant engineers on the planet. Despite decades of oversight, vulnerability reports and CVE counts continue to increasing. This is compounded by modern automated tooling, and AI-assisted vulnerability analysis that frequently [floods maintainers with false positives](https://www.youtube.com/watch?v=NnV_cWeoo5Q) and noise. If the best of the best struggle to manually handle raw memory, how can mere mortals juggle raw pointers safely in their heads? That precise question birthed Rust: a language designed to make memory safety the default rather than an afterthought.

## Safe vs. Unsafe Rust

Safe Rust guarantees memory safety at compile time: no [undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html) (UB), no data races, no dangling pointers. But hardware does **not** care about type systems. Sometimes you must talk directly to raw memory, foreign function interfaces (FFI) or hardware registers.

```
[ Safe Rust ] ---> ( Strict Guarantees )
       |
       v (Encapsulates)
[ Unsafe Rust ] -> ( Manual Assertions & Contracts )
       |
       v (Interfaces with)
[ C / FFI Code ] -> ( The Wild West )
       |
       v
[ Linux / Hardware ]
```

Rust uses this layered architecture to bridge high-level memory safety with low-level systems programming. The goal is to maximize performance and maintain control. In short, Safe Rust enforces memory safety rules, Unsafe Rust encapsulates operations the compiler cannot verify, FFI interfaces with external codebase, Linux is the foundation layer, which is [written in C](https://unix.stackexchange.com/questions/23634/why-are-most-linux-programs-written-in-c), requiring FFI.

In unsafe code, we are the borrow checker because the compiler cannot statically prove the [soundness](https://www.rustfaq.org/en/what-are-the-rules-for-unsafe-code-soundness/) of the code. We assert correctness manually, telling the compiler: "Trust me, I checked the invariants myself."

Unsafe does not mean broken. It simply means unverified by the compiler, shifting the *burden of proof* onto the human engineer.

## Unsafe Scope

As Jon Gjengset famously noted:
> When you have unsafe, the unsafety is not just about the code contained in the unsafe block. It is about the entire public visibility boundary of that code.

Look at the following example:
```rust
let buffer = b"hello";
let text = unsafe { std::str::from_utf8_unchecked(buffer) };
```

The buffer contains only valid UTF8 bytes, therefore the unsafe code is sound. But, what if the buffer gets corrupted, for example a cosmic ray switch the 'o' byte to `0xFF` or the buffer coming from an external source, containing invalid bytes? Suddenly, the same code is unsound. The example with the `b"hell\xff"` buffer runs without abort[^1] and prints a junk character. You could say that it is harmless, however violating invariants could lead to UB because Rust **assumes** valid UTF-8 and optimizes the code in that regard.

Generic code and closures, such as `BTreeMap` relying on `Ord` trait, create trust boundaries by depending on external implementations that might be flawed. While safe traits prioritize ergonomics, they **force** internal unsafe code to defend against incorrect trait implementation. Alternatively, marking a trait unsafe shifts the burden of correctness to the user, but Rust generally avoids this because, much like [async code](https://users.rust-lang.org/t/horrible-experience-with-async-today/122826), unsafety can easily become pervasive and *viral* across a codebase.

Foundational traits like `Send` and `Sync` are marked as unsafe because they cannot prepare against thread safety failures. To minimize manual implementation and widespread unsafety, these traits are automatically derived for composite types for better ergonomics. Marking trait with `unsafe` should be a thoroughly considered API design decision.

## Demystifying Unsafe

There is a harmful tendency in tech communities to treat the keyword unsafe like an [anti-pattern](https://www.reddit.com/r/rust/comments/y1u068/the_stigma_around_unsafe/). When automated scanners or casual audits spot unsafe in a codebase, it frequently triggers unneeded panic or community backlash against [unfounded](https://iliana.fyi/blog/software-supply-chain/) maintainers. It is not a binary choice of "good vs. bad", unsafe is direction-giving information. It explicitly flags where developers must focus their screening efforts.

The primary purpose of unsafe is to minimize the amount of code that requires manual auditing, isolating high-risk logic away from safe abstractions. When FFI is involved, context matters. An unsafe call hidden safely behind an internal implementation detail is vastly different from one directly exposed through a public API. Complex decisions like this should ideally be evaluated by vetting ecosystems such as `cargo-vet` rather than naive true or false flags. OSS maintainer burnout is already at critical levels. Witch hunts over the mere presence of unsafe keywords hurt communities. 

## How-to Unsafe

1. The absolute easiest way to handle unsafe code is not to write it, [YAGNI](https://en.wikipedia.org/wiki/You_aren't_gonna_need_it) principle. Stick to safe abstractions unless performance or low-level hardware access strictly demands otherwise. Use safe dependencies if available. Most of the time, the standard library is all you need!
2. Read the [Rustnomicon](https://doc.rust-lang.org/nightly/nomicon/intro.html#the-rustonomicon) before diving into raw pointers or custom unions, study the official [unsafe code guidelines](https://github.com/rust-lang/unsafe-code-guidelines).
3. **Document Your Invariants**: Always document every unsafe block using a clear "SAFETY:" comment detailing why the code is sound.
4. Integrate [Miri](https://github.com/rust-lang/miri) (to detect undefined behavior) and [ASAN](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html) (AddressSanitizer) into your CI pipelines.
5. When dealing with third-party crates containing unsafe code, leverage [cargo-vet](https://github.com/mozilla/cargo-vet) or [cargo-crev](https://github.com/crev-dev/cargo-crev) tools. If you feel the Qi, verify those unsafe code by yourself. You can clone the repository and run Miri, sanitizers and fuzzers on it. If you find a gap, submit a pull request to help the maintainers.

## Further Information

Videos:
- LowLevel - "how common is unsafe rust?": https://www.youtube.com/watch?v=z_RekEdKcfk
- Jon Gjengset - "Demystifying unsafe code": https://youtu.be/QAz-maaH0KM
- Let's Get Rusty - "The unsafe Rust survival guide": https://www.youtube.com/watch?v=91uStPBLibc
- Robert Seacord - "Unsafe Rust": https://www.youtube.com/watch?v=bTAEymCp7Ts

Reads:
- The Rustonomicon: https://doc.rust-lang.org/nomicon/intro.html
- Unsafe Rust: https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html
- A Brief Discussion on Invariants: https://rustmagazine.org/issue-4/a-brief-discussion-on-invariants-in-rust/
- Unsafe rust in the wild: https://rustfoundation.org/media/unsafe-rust-in-the-wild-notes-on-the-current-state-of-unsafe-rust/
- Common unsafe patterns: https://www.rustfaq.org/en/common-unsafe-patterns-in-rust-and-when-theyre-justified/

---

[^1]: The compiler is NOT turned off in the unsafe block. In the `from_utf8_unchecked` case, the compiler warns us about the invalid byte. I had to explicitly allow the `#[allow(invalid_from_utf8_unchecked)]` attribute to run the program. Therefore, the borrow checker runs, the types are checked, the ownership is enforced. The unsafe block *just* unlocks operations that are not allowed in Safe Rust.
