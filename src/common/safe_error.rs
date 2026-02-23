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
}
