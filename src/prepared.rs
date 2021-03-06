use super::{target, Apply, Chain, ConsumedToken, PartialApply, Take, TakeOwned, Token};
use std::marker::PhantomData;

/// Holds a single scoped modification into a copy of `T`.
/// The copy receives the modification lazily, and at the late stage
/// of `Prepared::apply`, the original value `T` is replaced by the
/// modified copy.
pub struct Prepared<OuterT, T, F, E> {
    inner: OuterT,
    f: F,
    _t: PhantomData<T>,
    _err: PhantomData<E>,
}

impl<'t, OuterT, T, FInner, E> Take<FInner, target::Function> for Prepared<OuterT, T, FInner, E> {
    fn take_ref(&self) -> &FInner {
        &self.f
    }

    fn take_mut(&mut self) -> &mut FInner {
        &mut self.f
    }
}

impl<'t, OuterT, T, FInner, E> TakeOwned<Token<'t, T>, target::Token>
    for Prepared<OuterT, T, FInner, E>
where
    OuterT: TakeOwned<Token<'t, T>, target::Token>,
{
    /// # Safety
    ///
    /// It is assumed that the caller has correctly used this method.
    unsafe fn take_owned(self) -> Token<'t, T> {
        self.inner.take_owned()
    }
}

impl<OuterT, T, F, E> Prepared<OuterT, T, F, E> {
    pub fn new(outer: OuterT, f: F) -> Self {
        Self {
            inner: outer,
            f,
            _t: PhantomData,
            _err: PhantomData,
        }
    }

    /// Chains this Prepared modification with another one, so that
    /// both copies may be lazily modified, and afther both
    /// doesn't indicate errors, they may be applied replaced into the
    /// original values.
    pub fn chain<A2>(self, a2: A2) -> Chain<Self, A2>
where {
        Chain::new(self, a2)
    }

    pub fn unchecked_cancel<'t>(self) -> Token<'t, T>
    where
        T: 't,
        OuterT: TakeOwned<Token<'t, T>, target::Token>,
    {
        unsafe { self.cancel() }
    }

    /// # Safety
    ///
    /// It is assumed that the caller has correctly used this method.
    pub unsafe fn cancel<'t>(self) -> Token<'t, T>
    where
        T: 't,
        OuterT: TakeOwned<Token<'t, T>, target::Token>,
    {
        self.inner.take_owned()
    }
}

impl<'t, OuterT, T, F, O, E> PartialApply<T, F, O, E> for Prepared<OuterT, T, F, E>
where
    OuterT: Take<T, target::Type> + Take<Token<'t, T>, target::Token>,
    F: FnOnce(&mut T) -> Result<O, E>,
    T: 't + Clone,
    OuterT: 't,
{
    fn get_next(&self) -> T {
        let next: &T = self.inner.take_ref();
        next.clone()
    }

    fn modify_next(mut next: T, f: F) -> Result<(O, T), E> {
        let o = (f)(&mut next)?;
        Ok((o, next))
    }

    fn replace(&mut self, next: T) {
        let current: &mut T = self.inner.take_mut();
        *current = next;
    }
}

unsafe impl<'t, OuterT, T, F, O, E> Apply<'t, T, F, O, E> for Prepared<OuterT, T, F, E>
where
    Self: PartialApply<T, F, O, E>,
    OuterT: Take<Token<'t, T>, target::Token> + TakeOwned<Token<'t, T>, target::Token>,
    T: 't,
    E: 't,
    F: 't + Clone,
    OuterT: 't,
{
    fn apply(mut self) -> crate::AllOrNone<'t, O, E, T> {
        let next = self.get_next();
        let f = self.f.clone();

        let (o, next) = match Self::modify_next(next, f) {
            Ok(v) => v,
            Err(e) => {
                // Safety:
                //
                // this is indicating that the mutation failed,
                // and also preventing further mutations
                let t = unsafe { self.inner.take_owned() };
                return Err((e, t));
            }
        };
        // Safety:
        //
        // only replace after the modifications were successful.
        // Also, after this, an `Ok` return is guaranteed
        self.replace(next);

        // Safety:
        //
        // this is indicating that the mutation was successful,
        // and also preventing further mutations
        let t = unsafe { self.inner.take_owned() };
        let consumed = ConsumedToken::from(t);
        Ok((o, consumed))
    }
}
