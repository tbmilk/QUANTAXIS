use super::ring_buffer::RingBuffer;
use super::IncrementalOperator;

#[derive(Debug, Clone, Default)]
pub struct SumState {
    pub sum: f64,
    pub count: u64,
}

pub struct Sum;

impl IncrementalOperator for Sum {
    type State = SumState;
    type Input = f64;
    type Output = f64;

    fn init() -> Self::State { SumState::default() }
    fn update(state: &mut Self::State, input: Self::Input) {
        state.sum += input; state.count += 1;
    }
    fn value(state: &Self::State) -> Self::Output { state.sum }
    fn expire(state: &mut Self::State, expired: Self::Input) {
        state.sum -= expired;
        state.count = state.count.saturating_sub(1);
    }
    fn merge(left: Self::State, right: Self::State) -> Self::State {
        SumState { sum: left.sum + right.sum, count: left.count + right.count }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CountState {
    pub count: u64,
}

pub struct Count;

impl IncrementalOperator for Count {
    type State = CountState;
    type Input = ();
    type Output = u64;

    fn init() -> Self::State { CountState::default() }
    fn update(state: &mut Self::State, _input: Self::Input) { state.count += 1; }
    fn value(state: &Self::State) -> Self::Output { state.count }
    fn merge(left: Self::State, right: Self::State) -> Self::State {
        CountState { count: left.count + right.count }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AvgState {
    pub sum: f64,
    pub count: u64,
}

pub struct Avg;

impl IncrementalOperator for Avg {
    type State = AvgState;
    type Input = f64;
    type Output = Option<f64>;

    fn init() -> Self::State { AvgState::default() }
    fn update(state: &mut Self::State, input: Self::Input) {
        state.sum += input; state.count += 1;
    }
    fn value(state: &Self::State) -> Self::Output {
        if state.count == 0 { None } else { Some(state.sum / state.count as f64) }
    }
    fn expire(state: &mut Self::State, expired: Self::Input) {
        state.sum -= expired;
        state.count = state.count.saturating_sub(1);
    }
    fn merge(left: Self::State, right: Self::State) -> Self::State {
        AvgState { sum: left.sum + right.sum, count: left.count + right.count }
    }
}

#[derive(Debug, Clone)]
pub struct MinState {
    pub min: f64,
    pub buffer: RingBuffer<f64>,
}

impl Default for MinState {
    fn default() -> Self {
        Self { min: f64::INFINITY, buffer: RingBuffer::new(64) }
    }
}

pub struct Min;

impl IncrementalOperator for Min {
    type State = MinState;
    type Input = f64;
    type Output = f64;

    fn init() -> Self::State { MinState::default() }
    fn update(state: &mut Self::State, input: Self::Input) {
        state.buffer.push(input);
        if input < state.min { state.min = input; }
    }
    fn value(state: &Self::State) -> Self::Output { state.min }
    fn expire(state: &mut Self::State, _expired: Self::Input) {
        state.min = state.buffer.iter().cloned().fold(f64::INFINITY, f64::min);
    }
    fn merge(left: Self::State, right: Self::State) -> Self::State {
        MinState {
            min: left.min.min(right.min),
            buffer: left.buffer,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaxState {
    pub max: f64,
    pub buffer: RingBuffer<f64>,
}

impl Default for MaxState {
    fn default() -> Self {
        Self { max: f64::NEG_INFINITY, buffer: RingBuffer::new(64) }
    }
}

pub struct Max;

impl IncrementalOperator for Max {
    type State = MaxState;
    type Input = f64;
    type Output = f64;

    fn init() -> Self::State { MaxState::default() }
    fn update(state: &mut Self::State, input: Self::Input) {
        state.buffer.push(input);
        if input > state.max { state.max = input; }
    }
    fn value(state: &Self::State) -> Self::Output { state.max }
    fn expire(state: &mut Self::State, _expired: Self::Input) {
        state.max = state.buffer.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    }
    fn merge(left: Self::State, right: Self::State) -> Self::State {
        MaxState {
            max: left.max.max(right.max),
            buffer: left.buffer,
        }
    }
}
