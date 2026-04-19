pub mod ring_buffer;
pub mod welford;
pub mod basic;
pub mod rolling;

pub use ring_buffer::*;
pub use welford::*;
pub use basic::*;
pub use rolling::*;

pub trait IncrementalOperator: Send + Sync {
    type State: Default + Clone + Send + Sync;
    type Input;
    type Output;

    fn init() -> Self::State;
    fn update(state: &mut Self::State, input: Self::Input);
    fn value(state: &Self::State) -> Self::Output;

    fn expire(_state: &mut Self::State, _expired: Self::Input) {}

    fn merge(left: Self::State, right: Self::State) -> Self::State;

    fn reset(state: &mut Self::State) {
        *state = Self::init();
    }
}

pub trait WindowedOperator: IncrementalOperator {
    fn window_size(&self) -> usize;
    fn is_full(state: &Self::State) -> bool;
    fn count(state: &Self::State) -> usize;
}
