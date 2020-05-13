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

use futures::task::{Context, Poll};
use std::pin::Pin;

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
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()});
            return async move {
                receiver.recv().unwrap()
            }
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*), $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name(move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($params),*);
            return async move {
                receiver.recv().unwrap()
            }
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*), $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($more_params),*);
            return async move {
                receiver.recv().unwrap()
            }
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
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()});
            $crate::CBStream::new(receiver)
        })()
    };
    // Callback as first parameter of function. This is similar to setTimeout() in javascript.
    ($func_name: ident(->($($c_params: ident),*), $($params: expr),*)) => {
        (|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name(move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($params),*);
            $crate::CBStream::new(receiver)
        })()
    };
    // Callback in the middle between other parameters of function. 
    ($func_name: ident($($params: expr),+, ->($($c_params: ident),*), $($more_params: expr),+)) => {
        (|| {
            let (sender, receiver) = std::sync::mpsc::channel();
            $func_name($($params),*, move |$($c_params),*| {sender.send(($($c_params),*)).unwrap()}, $($more_params),*);
            $crate::CBStream::new(receiver)
        })()
    };
}

/// A represent of callback function arguments which implement `futures::Stream` trait.
pub struct CBStream<T> {
    data_receiver: std::sync::mpsc::Receiver<T>,
    waker: Option<futures::task::Waker>
}

impl<T> CBStream<T> {
    pub fn new(reciever: std::sync::mpsc::Receiver<T>) -> CBStream<T> {
        CBStream {
            data_receiver: reciever,
            waker: None
        }
    }
}

impl<T> futures::Stream for CBStream<T> {
    type Item=T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.data_receiver.try_recv() {
            Ok(v) => Poll::Ready(Some(v)),
            Err(e) => {
                match e {
                    std::sync::mpsc::TryRecvError::Empty => {
                        self.waker = Some(cx.waker().clone());
                        Poll::Pending
                    },
                    std::sync::mpsc::TryRecvError::Disconnected => {
                        Poll::Ready(None)
                    }
                }
            }
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
}
