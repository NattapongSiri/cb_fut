//! Make a call to callback function without callback function.
//! 
//! This crate provide 2 macros to turn a call to function that require a callback parameter
//! for return values into a function call that return a `Future` of that values.
//! 
//! These macro introduce an indirection layer to glue callback to `Future` thus
//! it has some additional overhead.
//! 
//! # Limitation
//! - If the callback is not intended to return a value, don't use these two macros.
//! - If the function also return value, the returned value will be silently dropped.
//! - If the function take multiple callbacks to return value on different circumstance, don't use these two macros.
//! 
//! # What's new in version 0.2.0
//! - [once_blocked](macro.once_blocked.html) - which let user return value to function.
//! - [stream_blocked](macro.stream_blocked.html) - which let user return value to Stream.
//! - By default, it will use Rust standard channel. Now it also support `Crossbeam-channel`.
//! To use this feature, add features=["crossbeam"] to your `cargo.toml`. 
//! For example: 
//! ```toml
//! [dependencies]
//! cb_fut = {version = "^0.2", features = ["crossbeam"]}
//! ```

use futures::task::{Context, Poll};
use std::pin::Pin;

#[cfg(not(feature="crossbeam"))]
/// Utility function intended to be used internally. It will return a channel regarding to feature gate.
/// The default is to use standard channel.
pub fn channel<T>() -> (std::sync::mpsc::Sender<T>, std::sync::mpsc::Receiver<T>) {
    std::sync::mpsc::channel()
}

#[cfg(feature="crossbeam")]
/// Utility function intended to be used internally. It will return a channel regarding to feature gate.
/// With "crossbeam" feature gate specified, it will return crossbeam channel.
pub fn channel<T>() -> (crossbeam_channel::Sender<T>, crossbeam_channel::Receiver<T>) {
    crossbeam_channel::unbounded()
}

#[cfg(not(feature="crossbeam"))]
/// Internal type alias to reflect feature gate specified by user.
/// In this case, it is standard Sender from mpsc channel.
type Sender<T> = std::sync::mpsc::Sender<T>;

#[cfg(feature="crossbeam")]
/// Internal type alias to reflect feature gate specified by user.
/// In this case, it is crossbeam Sender crossbeam channel.
type Sender<T> = crossbeam_channel::Sender<T>;

/// Turn a function call that take a single callback function and return nothing into a function call
/// without callback that return a future value.
/// 
/// This macro can be used when the function invoke a callback only once to return a value.
/// If the callback will be called multiple times, use macro [stream](macro.stream.html) instead.
/// 
/// The function call signature need to have a placeholder for macro to identify a callback
/// parameter. To make it reflect to typical Rust syntax, the callback placeholder is
/// `->(a)` for a callback that take single parameter. The reason to choose `(a)` instead of
/// `|a|` is because the return Future from this macro will return a `(a)` tuple thus 
/// `->(a)` just like regular function return signature but with identifier instead of type.
/// 
/// Example usage:
/// ```rust
/// 
/// fn func(v: i32, cb: impl FnOnce(i32, i32)) {
///    std::thread::sleep(std::time::Duration::from_secs(2));
///    cb(v, v * 2)
/// }
/// 
/// let (a, b) = futures::executor::block_on(cb_fut::once!(func(2 + 3, ->(a, b))));
/// assert_eq!(5, a);
/// assert_eq!(10, b);
/// ```
#[macro_export]
macro_rules! once {
    // Typical callback style is to be last parameter. 
    ($func_name: ident($($params: expr),*, ->($($c_params: ident),*))) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()});
            return async move {
                receiver.recv().unwrap()
            }
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*), $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            $func_name(move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($params),*);
            return async move {
                receiver.recv().unwrap()
            }
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*), $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($more_params),*);
            return async move {
                receiver.recv().unwrap()
            }
        })()
    };
}

/// Turn a function call that take a single callback function to return a value
/// then wait for callback to return another value to continue it execution into a function
/// that return a `Future` which resolve to a struct that is `Deref` into a result and
/// it will automatically return value to a function when it is dropped. 
/// 
/// This macro will spawn a thread to execute the function while maintain the original
/// thread to return a Future to caller.
/// 
/// This macro can be used when the function invoke a callback only once to return a value.
/// If the callback will be called multiple times, use macro 
/// [stream_blocked](macro.stream_blocked.html) instead.
/// 
/// The function call signature need to have a placeholder for macro to identify a callback
/// parameter. To make it reflect to typical Rust syntax, the callback placeholder is
/// `->(a)->control_value` for a callback that take single parameter and return 
/// `control_value` as default callback return value. The reason to choose `(a)` instead of
/// `|a|` is because the return Future from this macro will return a struct that
/// can be `Deref` into `(a)` tuple thus `->(a)` just like regular function return signature 
/// but with identifier instead of type.
/// The `control_value` can be any valid Rust expression.
/// 
/// Example simple usage:
/// ```rust
/// fn func(v: i32, cb: impl FnOnce(i32, i32)->i32) {
///    std::thread::sleep(std::time::Duration::from_secs(2));
///    let controlled_value = cb(v, v * 2);
///    assert_eq!(controlled_value, 0i32);
///    dbg!(controlled_value);
/// }
/// 
/// // After the returned value goes out of scope, it'll print 0 on console.
/// let (a, b) = *futures::executor::block_on(cb_fut::once_blocked!(func(2 + 3, ->(a, b)->0i32)));
/// assert_eq!(5, a);
/// assert_eq!(10, b);
/// ```
/// 
/// Example of returning a value:
/// ```rust
/// fn func(v: i32, cb: impl FnOnce(i32, i32)->i32) {
///    std::thread::sleep(std::time::Duration::from_secs(2));
///    let controlled_value = cb(v, v * 2);
///    assert_eq!(controlled_value, 3i32);
///    dbg!(controlled_value);
/// }
/// 
/// let mut ret = futures::executor::block_on(cb_fut::once_blocked!(func(2 + 3, ->(a, b)->0i32)));
/// // It will print 3 instead of 0 in the console.
/// ret.return_value(3);
/// let (a, b) = *ret;
/// assert_eq!(5, a);
/// assert_eq!(10, b);
/// ```
#[macro_export]
macro_rules! once_blocked {
    // Typical callback style is to be a last parameter.
    ($func_name: ident($($params: expr),*, ->($($c_params: ident),*)->$c_ret: expr)) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            let (ret_sender, ret_receiver) = $crate::channel();
            let default_ret_sender = ret_sender.clone();
            let thread_handle = std::thread::spawn(move || {
                $func_name($($params),*, move |$($c_params),*| {
                    sender.send(($($c_params),*)).unwrap();
                    ret_receiver.recv().unwrap()
                })
            });
            return async move {
                let ($($c_params),*) = receiver.recv().unwrap();
                return $crate::CBBlockResult::new(
                    ($($c_params),*),
                    move |val| {ret_sender.send(val).unwrap()},
                    move || {default_ret_sender.send($c_ret).unwrap();},
                    Some(thread_handle)
                )
            };
            // return futures::select!(function = func => function, callback = cb => callback)
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*)->$c_ret: expr, $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            let (ret_sender, ret_receiver) = $crate::channel();
            let default_ret_sender = ret_sender.clone();
            let thread_handle = std::thread::spawn(move || {
                $func_name(move |$($c_params),*| {
                    sender.send(($($c_params),*)).unwrap();
                    ret_receiver.recv().unwrap()
                }, $($params),*,)
            });
            return async move {
                let ($($c_params),*) = receiver.recv().unwrap();
                return $crate::CBBlockResult::new(
                    ($($c_params),*),
                    move |val| {ret_sender.send(val).unwrap()},
                    move || {default_ret_sender.send($c_ret).unwrap();},
                    Some(thread_handle)
                )
            };
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*)->$c_ret: expr, $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = $crate::channel();
            let (ret_sender, ret_receiver) = $crate::channel();
            let default_ret_sender = ret_sender.clone();
            let thread_handle = std::thread::spawn(move || {
                $func_name($($params),*, move |$($c_params),*| {
                    sender.send(($($c_params),*)).unwrap();
                    ret_receiver.recv().unwrap()
                }, $($more_params),*,)
            });
            return async move {
                let ($($c_params),*) = receiver.recv().unwrap();
                return $crate::CBBlockResult::new(
                    ($($c_params),*),
                    move |val| {ret_sender.send(val).unwrap()},
                    move || {default_ret_sender.send($c_ret).unwrap();},
                    Some(thread_handle)
                )
            };
        })()
    };
}

/// Turn a function call that take a single callback and return nothing into a function call
/// without callback but return an implementation of `futures::Stream` called 
/// [CBStream](struct.CBStream.html).
/// 
/// If the callback will be called only once to return a value, consider using macro 
/// [once](macro.once.html) instead.
/// 
/// The function call signature need to have a placeholder for macro to identify a callback
/// parameter. To make it reflect to typical Rust syntax, the callback placeholder is
/// `->(a)` for a callback that take single parameter. The reason to choose `(a)` instead of
/// `|a|` is because the return Future from this macro will return a `(a)` tuple thus 
/// `->(a)` just like regular function return signature but with identifier instead of type.
/// 
/// Example usecase:
/// ```rust
/// use futures::stream::StreamExt;
/// fn func(u: i32, mut cb: impl FnMut(i32, i32), v: i32) {
///     for i in 0..5 {
///         cb(u + i, v * i)
///     }
/// }
/// let mut counter = 0;
/// 
/// futures::executor::block_on(cb_fut::stream!(func(2 * 3, ->(a, b), 2 + 3)).enumerate().for_each(|(i, fut)| {
///     counter += 1;
///     async move {
///         let (a, b) = fut;
///         assert_eq!(2 * 3 + i as i32, a);
///         assert_eq!((2 + 3) * i as i32, b);
///     }
/// }));
/// ```
#[macro_export]
macro_rules! stream {
    // Typical callback style is to be last parameter. 
    ($func_name: ident($($params: expr),*, ->($($c_params: ident),*))) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            $func_name($($params),*, move |$($c_params),*| {sender.unbounded_send(($($c_params),*)).unwrap()});
            $crate::CBStream::new(receiver)
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*), $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            $func_name(move |$($c_params),*| {sender.unbounded_send(($($c_params),*)).unwrap()}, $($params),*);
            $crate::CBStream::new(receiver)
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*), $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            $func_name($($params),*, move |$($c_params),*| {sender.unbounded_send(($($c_params),*)).unwrap()}, $($more_params),*);
            $crate::CBStream::new(receiver)
        })()
    };
}

/// Turn a function call that take a single callback and return nothing into a function call
/// without callback but return an implementation of `futures::Stream` called 
/// [CBStreamBlocked](struct.CBStreamBlocked.html).
/// 
/// This macro will spawn a thread to execute the function while maintain the original
/// thread to return a Future to caller.
/// 
/// If the callback will be called only once to return a value, consider using macro 
/// [once](macro.once.html) or [once_blocked](macro.once_blocked.html) instead.
/// 
/// The function call signature need to have a placeholder for macro to identify a callback
/// parameter. To make it reflect to typical Rust syntax, the callback placeholder is
/// `->(a)->b` for a callback that take single parameter and return some value. 
/// The reason to choose `(a)` instead of `|a|` is because the return Future from 
/// this macro will return a `(a)` tuple thus `->(a)` just like regular function return 
/// signature but with identifier instead of type.
/// The extra `->b` designate the default return expression. It will be automatically call 
/// when the generated result is dropped. If caller want to return different value,
/// it can be done by call method [return_value](struct.CBBlockResult.html#method.return_value).
/// It will prevent the default return from return value twice.
/// 
/// Example usecase:
/// ```rust
/// use futures::stream::StreamExt;
/// fn func(u: i32, mut cb: impl FnMut(i32, i32)->i32, v: i32) {
///     let mut j = 0;
///     while j < 5 {
///         j = cb(u + j, v * j)
///     }
/// }
/// let mut counter = 0;
/// 
/// futures::executor::block_on(cb_fut::stream_blocked!(func(2 * 3, ->(a, b)->0i32, 2 + 3)).enumerate().for_each(|(i, mut fut)| {
///     counter += 1;
///     async move {
///         let (a, b) = *fut;
///         assert_eq!(2 * 3 + i as i32, a);
///         assert_eq!((2 + 3) * i as i32, b);
///         fut.return_value(i as i32 + 1);
///     }
/// }));
/// ```
#[macro_export]
macro_rules! stream_blocked {
    // Typical callback style is to be last parameter. 
    ($func_name: ident($($params: expr),*, ->($($c_params: ident),*)->$c_ret: expr)) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let (ret_sender, ret_receiver) = $crate::channel();
            std::thread::spawn(move || {
                $func_name(
                    $($params),*, 
                    move |$($c_params),*| {
                        sender.unbounded_send(($($c_params),*)).unwrap();
                        let val = ret_receiver.recv().unwrap();
                        val
                    }
                )
            });
            Box::new($crate::CBStreamBlocked::new(ret_sender, receiver, $c_ret))
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*)->$c_ret: expr, $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let (ret_sender, ret_receiver) = $crate::channel();
            std::thread::spawn(move || {
                $func_name(
                    move |$($c_params),*| {
                        sender.unbounded_send(($($c_params),*)).unwrap();
                        let val = ret_receiver.recv().unwrap();
                        val
                    },
                    $($params),*
                )
            });
            Box::new($crate::CBStreamBlocked::new(ret_sender, receiver, $c_ret))
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*)->$c_ret: expr, $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let (ret_sender, ret_receiver) = $crate::channel();
            std::thread::spawn(move || {
                $func_name(
                    $($params),*,
                    move |$($c_params),*| {
                        sender.unbounded_send(($($c_params),*)).unwrap();
                        let val = ret_receiver.recv().unwrap();
                        val
                    },
                    $($more_params),*
                )
            });
            Box::new($crate::CBStreamBlocked::new(ret_sender, receiver, $c_ret))
        })()
    };
}

/// A represent of callback function arguments which implement `futures::Stream` trait.
pub struct CBStream<T> {
    data_receiver: futures::channel::mpsc::UnboundedReceiver<T>,
    waker: Option<futures::task::Waker>
}

impl<T> CBStream<T> {
    pub fn new(reciever: futures::channel::mpsc::UnboundedReceiver<T>) -> CBStream<T> {
        CBStream {
            data_receiver: reciever,
            waker: None
        }
    }
}

impl<T> futures::Stream for CBStream<T> {
    type Item=T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let data_receiver = &mut self.data_receiver;
        futures::pin_mut!(data_receiver);
        match data_receiver.poll_next(cx) {
            Poll::Ready(v) => {
                if let Some(v) = v {
                    Poll::Ready(Some(v))
                } else {
                    Poll::Ready(None)
                }
            },
            Poll::Pending => {
                self.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        } 
    }
}

/// An object that represent callback function arguments. It implement `futures::Stream` trait and return
/// value to the function by using [CBBlockResult](struct.CBBlockResult.html).
/// 
/// It can be obtain by using macro [stream_blocked](macro.stream_blocked!.html).
/// See [stream_blocked](macro.stream_blocked!.html) for example usage.
/// 
/// This struct shall be semi-transparent to user.
pub struct CBStreamBlocked<R, T> where R: 'static + Clone {
    data_receiver: futures::channel::mpsc::UnboundedReceiver<T>,
    ret_sender: crate::Sender<R>,
    default_return_value: R,
    waker: Option<futures::task::Waker>
}

impl<R, T> CBStreamBlocked<R, T> where R: 'static + Clone {
    /// You will likely use this macro [stream_blocked](macro.stream_blocked!.html) instead of
    /// trying to construct this struct using this method.
    /// 
    /// It require channel to communicate back and forth between the function and this struct.
    pub fn new(return_sender: crate::Sender<R>, reciever: futures::channel::mpsc::UnboundedReceiver<T>, default_ret_val: R) -> CBStreamBlocked<R, T> where R: 'static + Clone {
        CBStreamBlocked {
            data_receiver: reciever,
            default_return_value: default_ret_val,
            ret_sender: return_sender,
            waker: None
        }
    }
}

impl<R, T> futures::Stream for Box<CBStreamBlocked<R, T>> where R: 'static + Clone {
    type Item=CBBlockResult<R, T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // use futures::Stream;
        let data_receiver = &mut self.data_receiver;
        futures::pin_mut!(data_receiver);
        match data_receiver.poll_next(cx) {
            Poll::Ready(v) => {
                let ret_sender = self.ret_sender.clone();
                let default_sender = ret_sender.clone();
                let default_value = self.default_return_value.clone();
                if let Some(v) = v {
                    Poll::Ready(Some(
                        CBBlockResult::new(
                            v, 
                            move |val| {ret_sender.send(val).unwrap()}, 
                            move || default_sender.send(default_value).unwrap(), 
                            None
                        )
                    ))
                } else {
                    Poll::Ready(None)
                }
            }, Poll::Pending => {
                self.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        } 
    }
}

/// It mean that the value already return once and caller attempt to return something again.
#[derive(Debug, PartialEq)]
pub struct AlreadyReturnError;

impl std::fmt::Display for AlreadyReturnError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(fmt, "The caller has already return a value. It shall not return other value.")
    }
}

/// A structure that act as handle to retrieve result as well as return a value to function.
/// 
/// The struct implement `Deref` to let user get a direct access to result using tuple construct.
/// For example, `result.0` to access first result.
/// User can also using `Tuple` destructuring to get meaningful variable name such as
/// `let (a, b) = *result`.
/// 
/// The struct let user return value via method [return_value](struct.CBBlockResult.html#method.return_value).
/// It can be called only once. If user call it more than once, it will return Error 
/// [AlreadyReturnError](struct.AlreadyReturnError.html)
/// 
/// The struct hold the default return value which will be used to return to value to function
/// when it is dropped. However, calling [return_value](struct.CBBlockResult.html#method.return_value)
/// once will prevent returning default value.
/// 
/// If the result is obtain from macro [once_blocked](macro.once_blocked!.html), when this result
/// is dropped, it'll block current thread waiting until the function is completed.
/// 
/// If the result is obtain from `Stream` instance, it will never block.
pub struct CBBlockResult<R, T> {
    result: T,
    return_fn: Option<Box<dyn FnOnce(R)>>,
    default_return: Option<Box<dyn FnOnce()>>,
    func_handle: Option<std::thread::JoinHandle<()>>
}

/// Get a reference to underlying result.
impl<R, T> core::ops::Deref for CBBlockResult<R, T> {
    type Target=T;

    fn deref(&self) -> &T {
        &self.result
    }
}

impl<R, T> CBBlockResult<R, T> where R: 'static {
    /// User shall never need to call this function.
    /// Instead, user shall use macro, such as [once](macro.once_blocked!.html) to get a future
    /// that resolve into this object.
    pub fn new<F, FR>(result: T, caller_return_fn: FR, default_return: F, handle: Option<std::thread::JoinHandle<()>>) -> CBBlockResult<R, T> where F: 'static + FnOnce(), FR: 'static + FnOnce(R) {
        CBBlockResult {
            result,
            return_fn: Some(Box::new(caller_return_fn)),
            default_return: Some(Box::new(default_return)),
            func_handle: handle
        }
    }

    /// Return a value to the function and cancel out the default return value.
    pub fn return_value(&mut self, value: R) -> Result<(), AlreadyReturnError> {
        if self.return_fn.is_some() && self.default_return.is_some() {
            self.default_return.take();
            let ret_fn = self.return_fn.take().unwrap();
            (ret_fn)(value);
            Ok(())
        } else {
            Err(AlreadyReturnError)
        }
    }
}

/// Implement `Drop` to return the default control variable to original function is run to complete.
impl<R, T> Drop for CBBlockResult<R, T> {
    fn drop(&mut self) {
        if self.default_return.is_some() {
            let default_return = self.default_return.take().unwrap();
            (default_return)();
        }

        if self.func_handle.is_some() {
            // ensure that the thread is gracefully shutdown 
            let func_handle = self.func_handle.take().unwrap();
            func_handle.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_once_postfix() {
        fn func(v: i32, cb: impl FnOnce(i32, i32)) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            cb(v, v * 2)
        }
        let (a, b) = futures::executor::block_on(once!(func(2 + 3, ->(a, b))));
        assert_eq!(5, a);
        assert_eq!(10, b);
    }

    #[test]
    fn test_once_prefix() {
        fn func(cb: impl FnOnce(i32, i32), v: i32) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            cb(v, v * 2)
        }
        let (a, b) = futures::executor::block_on(once!(func(->(a, b), 2 + 3)));
        assert_eq!(5, a);
        assert_eq!(10, b);
    }

    #[test]
    fn test_once_infix() {
        fn func(u: i32, cb: impl FnOnce(i32, i32), v: i32) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            cb(u, v)
        }
        let (a, b) = futures::executor::block_on(once!(func(1, ->(a, b), 2 + 3)));
        assert_eq!(1, a);
        assert_eq!(5, b);
    }

    #[test]
    fn test_once_postfix_no_args() {
        fn func(_v: i32, cb: impl FnOnce()) {
            std::thread::sleep(std::time::Duration::from_secs(2));
            cb()
        }
        futures::executor::block_on(once!(func(2 + 3, -> ())));
    }

    #[test]
    fn test_once_blocked_postfix() {
        fn func(v: i32, cb: impl FnOnce(i32, i32) -> i32) {
            if cb(v, v * 2) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        let mut ret = futures::executor::block_on(once_blocked!(func(2 + 3, ->(a, b) -> 0i32)));
        let (a, b) = *ret;
        assert_eq!(5, a);
        assert_eq!(10, b);
        ret.return_value(0).unwrap();
    }

    #[test]
    fn test_once_blocked_default_postfix() {
        fn func(v: i32, cb: impl FnOnce(i32, i32) -> i32) {
            if cb(v, v * 2) == 0i32 {
                dbg!("Ok !");
            } else {
                dbg!("Default shutdown..");
            }
        }
        let ret = futures::executor::block_on(once_blocked!(func(2 + 3, ->(a, b) -> 1i32)));
        let (a, b) = *ret;
        assert_eq!(5, a);
        assert_eq!(10, b);
    }

    #[test]
    fn test_once_blocked_default_postfix_no_args() {
        fn func(_v: i32, cb: impl FnOnce() -> i32) {
            if cb() == 3i32 {
                dbg!("Ok !");
            } else {
                panic!("Invalid return value")
            }
        }
        futures::executor::block_on(once_blocked!(func(2 + 3, ->() -> {3i32})));
    }

    #[test]
    fn test_once_blocked_postfix_with_logic() {
        fn func(v: i32, cb: impl FnOnce(i32, i32) -> i32) {
            if cb(v, v * 2) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        let mut ret = futures::executor::block_on(once_blocked!(func(2 + 3, ->(a, b) -> 0i32)));
        let (a, b) = *ret;
        assert_eq!(5, a);
        assert_eq!(10, b);
        if a + b == 15 && a * b == 50 {
            ret.return_value(0).unwrap();
        }
        assert_eq!(ret.return_value(0).unwrap_err(), super::AlreadyReturnError);
    }

    #[test]
    fn test_once_blocked_prefix() {
        fn func(cb: impl FnOnce(i32, i32) -> i32, v: i32) {
            if cb(v, v * 2) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        let mut ret = futures::executor::block_on(once_blocked!(func(->(a, b) -> 0i32, 2 + 3)));
        let (a, b) = *ret;
        assert_eq!(5, a);
        assert_eq!(10, b);
        ret.return_value(0).unwrap();
    }

    #[test]
    fn test_once_blocked_default_prefix() {
        fn func(cb: impl FnOnce(i32, i32) -> i32, v: i32) {
            if cb(v, v * 2) == 0i32 {
                dbg!("Ok !");
            } else {
                dbg!("Default shutdown..");
            }
        }
        let ret = futures::executor::block_on(once_blocked!(func(->(a, b) -> 1i32, 2 + 3)));
        let (a, b) = *ret;
        assert_eq!(5, a);
        assert_eq!(10, b);
    }

    #[test]
    fn test_once_blocked_default_prefix_no_args() {
        fn func(cb: impl FnOnce() -> i32, _v: i32) {
            if cb() == 3i32 {
                dbg!("Ok !");
            } else {
                panic!("Invalid return value")
            }
        }
        futures::executor::block_on(once_blocked!(func(->() -> {3i32}, 2 + 3)));
    }

    #[test]
    fn test_once_blocked_infix() {
        fn func(u: i32, cb: impl FnOnce(i32, i32) -> i32, v: i32) {
            if cb(u + v, u * v) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        let mut ret = futures::executor::block_on(once_blocked!(func(2i32, ->(a, b) -> 0i32, 2 + 3)));
        let (a, b) = *ret;
        assert_eq!(7, a);
        assert_eq!(10, b);
        ret.return_value(0).unwrap();
    }

    #[test]
    fn test_once_blocked_default_infix() {
        fn func(u: i32, cb: impl FnOnce(i32, i32) -> i32, v: i32) {
            if cb(u + v, u * v) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        let ret = futures::executor::block_on(once_blocked!(func(2i32, ->(a, b) -> 0i32, 2 + 3)));
        let (a, b) = *ret;
        assert_eq!(7, a);
        assert_eq!(10, b);
    }

    #[test]
    fn test_once_blocked_default_infix_no_args() {
        fn func(u: i32, cb: impl FnOnce(i32, i32) -> i32, v: i32) {
            if cb(u + v, u * v) == 0i32 {
                dbg!("Ok !");
            } else {
                panic!("Something wrong")
            }
        }
        futures::executor::block_on(once_blocked!(func(2i32, ->(a, b) -> 0i32, 2 + 3)));
    }

    #[test]
    fn test_stream_postfix() {
        use futures::stream::StreamExt;
        fn func(v: i32, mut cb: impl FnMut(i32, i32)) {
            for i in 0..5 {
                cb(v, v * i)
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream!(func(2 + 3, ->(a, b))).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let (a, b) = fut;
                assert_eq!(5, a);
                assert_eq!(5 * i as i32, b);
            }
        }));
        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_prefix() {
        use futures::stream::StreamExt;
        fn func(mut cb: impl FnMut(i32, i32), v: i32) {
            for i in 0..5 {
                cb(v, v * i)
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream!(func(->(a, b), 2 + 3)).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let (a, b) = fut;
                assert_eq!(5, a);
                assert_eq!(5 * i as i32, b);
            }
        }));

        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_infix() {
        use futures::stream::StreamExt;
        fn func(u: i32, mut cb: impl FnMut(i32, i32), v: i32) {
            for i in 0..5 {
                cb(u + i, v * i)
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream!(func(2 * 3, ->(a, b), 2 + 3)).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let (a, b) = fut;
                assert_eq!(2 * 3 + i as i32, a);
                assert_eq!((2 + 3) * i as i32, b);
            }
        }));

        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_prefix_no_args() {
        use futures::stream::StreamExt;
        fn func(mut cb: impl FnMut(), _v: i32) {
            for _ in 0..5 {
                cb()
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream!(func(->(), 2 + 3)).for_each(|_| {
            counter += 1;
            // need to return future per requirement of `for_each`
            async {}
        }));

        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_blocked_postfix() {
        use futures::stream::StreamExt;
        fn func(v: i32, mut cb: impl FnMut(i32, i32)->i32) {
            for i in 0..5 {
                cb(v, v * i);
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream_blocked!(func(2 + 3, ->(a, b)->0i32)).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let mut result = fut;
                result.return_value(i as i32).unwrap();
                let (a, b) = *result;
                assert_eq!(5, a);
                assert_eq!(5 * i as i32, b);
            }
        }));
        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_blocked_prefix() {
        use futures::stream::StreamExt;
        fn func(mut cb: impl FnMut(i32, i32)->i32, v: i32) {
            for i in 0..5 {
                cb(v, v * i);
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream_blocked!(func(->(a, b)->0i32, 2 + 3)).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let mut result = fut;
                result.return_value(i as i32).unwrap();
                let (a, b) = *result;
                assert_eq!(5, a);
                assert_eq!(5 * i as i32, b);
            }
        }));
        assert_eq!(5, counter);
    }

    #[test]
    fn test_stream_blocked_infix() {
        use futures::stream::StreamExt;
        fn func(u: i32, mut cb: impl FnMut(i32, i32)->i32, v: i32) {
            let mut j = 0;
            for _ in 0..5 {
                j = cb(u + j, v * j);
            }
        }
        let mut counter = 0;
        
        futures::executor::block_on(stream_blocked!(func(2, ->(a, b)->0i32, 2 + 3)).enumerate().for_each(|(i, fut)| {
            counter += 1;
            async move {
                let mut result = fut;
                result.return_value(i as i32 + 1i32).unwrap();
                let (a, b) = *result;
                assert_eq!(i as i32 + 2i32, a);
                assert_eq!((3i32 + 2i32) * i as i32, b);
            }
        }));
        assert_eq!(5, counter);
    }
}
