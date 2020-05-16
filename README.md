# cb_fut
This crate provide utility macros to convert a call to function that need a callback to handle return values into a function that either return a `Future` that resolve to values or a `Stream` that yield values. It introduce extra syntax `-> ()` into functional call signature.

## Limitation
Following are limitations of this crate.
- If the callback is not intended for return a value, don't use these two macros. It'll break the function.
- If the function also return value, the returned value will be silently dropped.
- The macro support only single callback conversion. If the function take more than one callbacks to return value on different circumstances, it cannot be use.
- It will be slower compare to using original callback. This is because it require some kind of indirection to direct a callback through channel into a `Future`.

## How to use
This crate provide four macros in two categories.
### Non-blocking
It use `futures::channel::unbound` to communicate between function callback and `Future`. There is two macros in this category.
1. `once` - for convert a call to a function with a callback into a `Future` that resolve to values which used to be return as callback parameters.
1. `stream` - for convert a call to a function with a callback into a `Stream` that yield values which usually pass to callback as parameters.
### Blocking
It use two channels to keep sync between each `CBBlockResult` and callback. It spawn a new thread to execute the function and block the function waiting for caller to execute method `return_value` on `CBBlockResult` or until the `CBBlockResult` is dropped. There's two macros in this category.
1. `once_blocked` - to get a `Future` of result from a call to a function with a callback that return something back to function. It is different from `once` macro on that it block the function until the result is drop or method `return_value` is explicitly called. This macro will join the thread executing function when the `CBBlockResult` is drop.
1. `stream_blocked` - to get a `CBStreamBlocked` that yield a result, `CBBlockResult`, contains values similar to what was send to a callback. The result is different from regular `stream` macro on that it block a function until the result is drop or explicitly call `return_value` method.

The main difference between the two is `once` and `once_blocked` shall be used if a callback will be called exactly once while `stream` and `stream_blocked` shall be used if a callback will be called multiple times.

These two category need a special syntax to identify number of expected return variables. The syntax is `-> ()` and `->()->v`. The prior resemble the function signature for returning type, e.g. `Fn(i32) -> ()`. The later is extended from prior on that the callback itself return something back to the function which might affect execution of that function. The `->v` part will be used as default value when caller doesn't call `return_value` method. This is to guarantee that the function will receive value back to resume the execution.

These conversion result in `Future` or `Stream` that will resolve to tuple of values. See examples below.

### Examples
Turn a function with callback on last parameter into a future.
```rust
// A function that need a callback
fn func(v: i32, cb: impl FnOnce(i32, i32)) {
    std::thread::sleep(std::time::Duration::from_secs(2));
    cb(v, v * 2)
}

// Use once! to convert the call to `func` to return a `Future` instead.
// We use ->(a, b) to tell macro that `Future` shall return two variables.
let (a, b) = futures::executor::block_on(once!(func(2 + 3, ->(a, b))));

assert_eq!(5, a);
assert_eq!(10, b);
```
The callback can be in anywhere in function signature. The callback placeholder just need to reflect that too.
```rust
// A function that put callback in the middle between other two parameters
fn func(u: i32, cb: impl FnOnce(i32, i32), v: i32) {
    std::thread::sleep(std::time::Duration::from_secs(2));
    cb(u, v)
}
// We use `->(a, b)` between 1, and, 2 + 3 to tell macro that this parameter is a callback and it take 2 parameters.
let (a, b) = futures::executor::block_on(once!(func(1, ->(a, b), 2 + 3)));
assert_eq!(1, a);
assert_eq!(5, b);
```
In case the callback take no argument, we need to put `-> ()`
```rust
// a function that take no arguments callback
fn func(_v: i32, cb: impl FnOnce()) {
    std::thread::sleep(std::time::Duration::from_secs(2));
    cb()
}
// A callback placeholder with no argument
futures::executor::block_on(once!(func(2 + 3, -> ())));
```
If callback will be called multiple times, use `stream!`
```rust
use futures::stream::StreamExt;
// A function that take callback as first argument.
// It'll call callback 5 times with two arguments, an original value and the original value times number of called.
fn func(mut cb: impl FnMut(i32, i32), v: i32) {
    for i in 0..5 {
        cb(v, v * i)
    }
}
let mut counter = 0;

// `stream!` will return `CBStream` which implement `Stream` trait. We use `enumerate` and `for_each` from `StreamExt` trait to iterate over each values tuples that suppose to be passed to callback function. 
// The `for_each` method signature require a return value of type `Future` for given callback. The final return value from `for_each` is a single consolidated `Future` which when resolve, all `Future`s inside it are all resolved.
futures::executor::block_on(stream!(func(->(a, b), 2 + 3)).enumerate().for_each(|(i, fut)| {
    counter += 1;
    async move {
        let (a, b) = fut;
        assert_eq!(5, a);
        assert_eq!(5 * i as i32, b);
    }
}));

assert_eq!(5, counter);
```
The callback which has side effect on the function after callback is executed.
```rust
fn func(v: i32, cb: impl FnOnce(i32, i32) -> i32) {
    if cb(v, v * 2) == 0i32 {
        dbg!("Ok !");
    } else {
        panic!("Something wrong")
    }
}
let mut ret = futures::executor::block_on(once_blocked!(func(2 + 3, ->(a, b) -> 1i32)));
let (a, b) = *ret;
assert_eq!(5, a);
assert_eq!(10, b);
if a + b == 15 && a * b == 50 {
    ret.return_value(0).unwrap();
}
assert_eq!(ret.return_value(0).unwrap_err(), super::AlreadyReturnError);
```
The callback which control Stream control flow
```rust
use futures::stream::StreamExt;
fn func(u: i32, mut cb: impl FnMut(i32, i32)->i32, v: i32) {
    let mut j = 0;
    while j < 5 {
        j = cb(u + j, v * j)
    }
}
let mut counter = 0;

futures::executor::block_on(cb_fut::stream_blocked!(func(2 * 3, ->(a, b)->0i32, 2 + 3)).enumerate().for_each(|(i, mut fut)| {
    counter += 1;
    async move {
        let (a, b) = *fut;
        assert_eq!(2 * 3 + i as i32, a);
        assert_eq!((2 + 3) * i as i32, b);
        fut.return_value(i as i32 + 1);
    }
}));
```