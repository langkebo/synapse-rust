use std::fmt;
use std::ops::ControlFlow;

#[derive(Debug, Clone)]
pub struct SafeResult<T, E = SafeError> {
    inner: Result<T, E>,
}

impl<T, E> SafeResult<T, E>
where
    E: fmt::Display,
{
    #[inline]
    pub fn new(result: Result<T, E>) -> Self {
        Self { inner: result }
    }

    #[inline]
    pub fn map<U, F>(self, f: F) -> SafeResult<U, E>
    where
        F: FnOnce(T) -> U,
    {
        SafeResult::new(self.inner.map(f))
    }

    #[inline]
    pub fn map_err<F, E2>(self, f: F) -> SafeResult<T, E2>
    where
        F: FnOnce(E) -> E2,
    {
        SafeResult::new(self.inner.map_err(f))
    }

    #[inline]
    pub fn and_then<U, F>(self, f: F) -> SafeResult<U, E>
    where
        F: FnOnce(T) -> SafeResult<U, E>,
    {
        match self.inner {
            Ok(t) => f(t).inner.into(),
            Err(e) => Err(e).into(),
        }
    }

    #[inline]
    pub fn or_else<F, E2>(self, f: F) -> SafeResult<T, E2>
    where
        F: FnOnce(E) -> SafeResult<T, E2>,
    {
        match self.inner {
            Ok(t) => Ok(t).into(),
            Err(e) => f(e).inner.into(),
        }
    }

    #[inline]
    pub fn unwrap_or(self, default: T) -> T {
        self.inner.unwrap_or(default)
    }

    #[inline]
    pub fn unwrap_or_else<F>(self, f: F) -> T
    where
        F: FnOnce(E) -> T,
    {
        self.inner.unwrap_or_else(f)
    }

    #[inline]
    pub fn is_ok(&self) -> bool {
        self.inner.is_ok()
    }

    #[inline]
    pub fn is_err(&self) -> bool {
        self.inner.is_err()
    }

    #[inline]
    pub fn ok(self) -> Option<T> {
        self.inner.ok()
    }

    #[inline]
    pub fn err(self) -> Option<E> {
        self.inner.err()
    }

    #[inline]
    pub fn as_ref(&self) -> SafeResult<&T, &E> {
        match self.inner.as_ref() {
            Ok(t) => Ok(t),
            Err(e) => Err(e),
        }
    }

    #[inline]
    pub fn as_mut(&mut self) -> SafeResult<&mut T, &mut E> {
        match self.inner.as_mut() {
            Ok(t) => Ok(t),
            Err(e) => Err(e),
        }
    }
}

impl<T, E> From<Result<T, E>> for SafeResult<T, E> {
    #[inline]
    fn from(result: Result<T, E>) -> Self {
        SafeResult::new(result)
    }
}

impl<T, E> From<SafeResult<T, E>> for Result<T, E> {
    #[inline]
    fn from(safe: SafeResult<T, E>) -> Self {
        safe.inner
    }
}

#[derive(Debug, Clone)]
pub enum SafeError {
    PoisonedLock(String),
    ParseError(String),
    ValidationError(String),
    ConversionError(String),
    OperationFailed(String),
}

impl fmt::Display for SafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SafeError::PoisonedLock(location) => write!(f, "Poisoned lock at {}", location),
            SafeError::ParseError(details) => write!(f, "Parse error: {}", details),
            SafeError::ValidationError(details) => write!(f, "Validation error: {}", details),
            SafeError::ConversionError(details) => write!(f, "Conversion error: {}", details),
            SafeError::OperationFailed(details) => write!(f, "Operation failed: {}", details),
        }
    }
}

impl std::error::Error for SafeError {}

impl<T> From<SafeResult<T>> for SafeError {
    fn from(safe: SafeResult<T>) -> Self {
        SafeError::OperationFailed("SafeResult contained an error".to_string())
    }
}

pub trait Safeunwrap<T> {
    fn safe_unwrap(self, context: &str) -> T;
    fn safe_expect(self, context: &str) -> T;
}

impl<T, E> Safeunwrap<T> for Result<T, E>
where
    E: fmt::Display,
{
    fn safe_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|e| {
            panic!("{}: {}", context, e)
        })
    }

    fn safe_expect(self, context: &str) -> T {
        self.expect(context)
    }
}

impl<T> Safeunwrap<T> for Option<T> {
    fn safe_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|| {
            panic!("{}: Expected Some(_) but got None", context)
        })
    }

    fn safe_expect(self, context: &str) -> T {
        self.expect(context)
    }
}

#[macro_export]
macro_rules! safe {
    ($result:expr) => {
        SafeResult::new($result)
    };
}

#[macro_export]
macro_rules! try_safe {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return Err(SafeError::from(e)),
        }
    };
    ($expr:expr, $context:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                tracing::error!(context = $context, error = %e, "Operation failed");
                return Err(SafeError::OperationFailed(format!("{}: {}", $context, e)));
            }
        }
    };
}

#[inline]
pub fn safe_lock<R, F, M: std::ops::Deref<Target = parking_lot::RwLock<R>>>>(
    guard: &M,
    context: &str,
    f: F,
) -> SafeResult<R>
where
    F: FnOnce(&R) -> R,
{
    match guard.read() {
        Ok(guard) => Ok(f(&guard)),
        Err(_) => Err(SafeError::PoisonedLock(context.to_string())),
    }
}

#[inline]
pub fn safe_write_lock<R, F, M: std::ops::Deref<Target = parking_lot::RwLock<R>>>>(
    guard: &M,
    context: &str,
    f: F,
) -> SafeResult<R>
where
    F: FnOnce(&mut R) -> R,
{
    match guard.write() {
        Ok(mut guard) => Ok(f(&mut guard)),
        Err(_) => Err(SafeError::PoisonedLock(context.to_string())),
    }
}

#[inline]
pub fn safe_mutex_lock<R, F, M: std::ops::Deref<Target = std::sync::Mutex<R>>>>(
    guard: &M,
    context: &str,
    f: F,
) -> SafeResult<R>
where
    F: FnOnce(&R) -> R,
{
    match guard.lock() {
        Ok(guard) => Ok(f(&guard)),
        Err(_) => Err(SafeError::PoisonedLock(context.to_string())),
    }
}

#[inline]
pub fn safe_poison_guard<T, F>(result: std::sync::PoisonError<T>, context: &str) -> SafeError {
    SafeError::PoisonedLock(format!("{}: {:?}", context, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_result_creation() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(42));

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("test".to_string())));
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_result_map() {
        let result = SafeResult::new(Ok(42));
        let mapped = result.map(|x| x * 2);
        assert_eq!(mapped.ok(), Some(84));
    }

    #[test]
    fn test_safe_result_and_then() {
        let result = SafeResult::new(Ok(42));
        let chained = result.and_then(|x| SafeResult::new(Ok(x * 2)));
        assert_eq!(chained.ok(), Some(84));

        let result = SafeResult::new(Err(SafeError::ParseError("test".to_string())));
        let chained = result.and_then(|x| SafeResult::new(Ok(x * 2)));
        assert!(chained.is_err());
    }

    #[test]
    fn test_safe_unwrap_or() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        assert_eq!(result.unwrap_or(0), 42);

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("test".to_string())));
        assert_eq!(result.unwrap_or(0), 0);
    }

    #[test]
    fn test_safe_lock() {
        use parking_lot::RwLock;
        let lock = RwLock::new(42);

        let result = safe_lock(&lock, "test", |v| *v * 2);
        assert_eq!(result.ok(), Some(84));
    }

    #[test]
    fn test_safe_error_display() {
        let err = SafeError::PoisonedLock("test_location".to_string());
        assert_eq!(err.to_string(), "Poisoned lock at test_location");

        let err = SafeError::ParseError("invalid input".to_string());
        assert_eq!(err.to_string(), "Parse error: invalid input");

        let err = SafeError::ValidationError("field required".to_string());
        assert_eq!(err.to_string(), "Validation error: field required");

        let err = SafeError::ConversionError("type mismatch".to_string());
        assert_eq!(err.to_string(), "Conversion error: type mismatch");

        let err = SafeError::OperationFailed("network error".to_string());
        assert_eq!(err.to_string(), "Operation failed: network error");
    }

    #[test]
    fn test_safe_result_map_err() {
        let result = SafeResult::new(Err::<i32, _>(SafeError::ParseError("original".to_string())));
        let mapped = result.map_err(|e| SafeError::ValidationError(format!("validated: {}", e)));
        assert!(mapped.is_err());
        let err = mapped.err().unwrap();
        match err {
            SafeError::ValidationError(msg) => assert!(msg.contains("validated:")),
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_safe_result_or_else() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        let handled = result.or_else(|_| SafeResult::new(Ok(0)));
        assert_eq!(handled.ok(), Some(42));

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("error".to_string())));
        let handled = result.or_else(|_| SafeResult::new(Ok(100)));
        assert_eq!(handled.ok(), Some(100));
    }

    #[test]
    fn test_safe_result_unwrap_or_else() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        let value = result.unwrap_or_else(|e| panic!("Should not be called: {}", e));
        assert_eq!(value, 42);

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("error".to_string())));
        let value = result.unwrap_or_else(|e| e.to_string().len() as i32);
        assert_eq!(value, 18); // "Parse error: error".len() = 18
    }

    #[test]
    fn test_safe_result_as_ref() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        let ref_result = result.as_ref();
        assert_eq!(ref_result.ok(), Some(&42));

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("error".to_string())));
        let ref_result = result.as_ref();
        assert!(ref_result.is_err());
    }

    #[test]
    fn test_safe_result_as_mut() {
        let mut result: SafeResult<i32> = SafeResult::new(Ok(42));
        let mut_ref = result.as_mut();
        assert_eq!(mut_ref.ok(), Some(&mut 42));
        *mut_ref.as_mut().ok().unwrap() *= 2;
        let result2: SafeResult<i32> = result;
        assert_eq!(result2.ok(), Some(84));
    }

    #[test]
    fn test_safe_result_err() {
        let result: SafeResult<i32> = SafeResult::new(Ok(42));
        assert!(result.err().is_none());

        let result: SafeResult<i32> = SafeResult::new(Err(SafeError::ParseError("error".to_string())));
        assert!(result.err().is_some());
    }

    #[test]
    fn test_safe_result_from_result() {
        let r: Result<i32, SafeError> = Ok(42);
        let safe: SafeResult<i32> = r.into();
        assert!(safe.is_ok());

        let r: Result<i32, SafeError> = Err(SafeError::OperationFailed("failed".to_string()));
        let safe: SafeResult<i32> = r.into();
        assert!(safe.is_err());
    }

    #[test]
    fn test_safe_result_into_result() {
        let safe = SafeResult::new(Ok(42));
        let r: Result<i32, SafeError> = safe.into();
        assert_eq!(r.unwrap(), 42);

        let safe = SafeResult::new(Err(SafeError::ParseError("error".to_string())));
        let r: Result<i32, SafeError> = safe.into();
        assert!(r.is_err());
    }

    #[test]
    fn test_safe_write_lock() {
        use parking_lot::RwLock;
        let lock = RwLock::new(42);

        let result = safe_write_lock(&lock, "test", |v| {
            *v *= 2;
            *v
        });
        assert_eq!(result.ok(), Some(84));
    }

    #[test]
    fn test_safe_mutex_lock() {
        use std::sync::Mutex;
        let lock = Mutex::new(42);

        let result = safe_mutex_lock(&lock, "test", |v| *v * 2);
        assert_eq!(result.ok(), Some(84));
    }

    #[test]
    fn test_safe_poison_guard() {
        use std::sync::PoisonError;
        let lock = std::sync::Mutex::new(42);
        let poison = lock.lock().unwrap_err();
        let err = safe_poison_guard(poison, "mutex_test");
        match err {
            SafeError::PoisonedLock(msg) => assert!(msg.contains("mutex_test")),
            _ => panic!("Expected PoisonedLock"),
        }
    }

    #[test]
    fn test_safe_unwrap_option() {
        let opt: Option<i32> = Some(42);
        assert_eq!(opt.safe_unwrap("test"), 42);

        let opt: Option<i32> = None;
        let result = std::panic::catch_unwind(|| {
            opt.safe_unwrap("test");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_expect_option() {
        let opt: Option<i32> = Some(42);
        assert_eq!(opt.safe_expect("test"), 42);
    }

    #[test]
    fn test_safe_unwrap_result() {
        let res: Result<i32, &str> = Ok(42);
        assert_eq!(res.safe_unwrap("test"), 42);

        let res: Result<i32, &str> = Err("error");
        let result = std::panic::catch_unwind(|| {
            res.safe_unwrap("test");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_macro() {
        let result = safe!(Ok(42));
        assert!(result.is_ok());

        let result = safe!(Err::<i32, _>("error"));
        assert!(result.is_err());
    }

    #[test]
    fn test_try_safe_macro() {
        fn divide(a: i32, b: i32) -> Result<i32, &'static str> {
            if b == 0 {
                Err("division by zero")
            } else {
                Ok(a / b)
            }
        }

        let result = try_safe!(divide(10, 2));
        assert_eq!(result, 5);

        let result = try_safe!(divide(10, 0));
        assert!(result.is_err());
    }
}
