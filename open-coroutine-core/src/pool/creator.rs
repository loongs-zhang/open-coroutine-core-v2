use crate::pool::CoroutinePool;
use crate::scheduler::listener::Listener;
use crate::scheduler::SchedulableCoroutine;

#[derive(Debug)]
pub(crate) struct CoroutineCreator {
    pool: &'static CoroutinePool,
}

impl CoroutineCreator {
    pub(crate) fn new(pool: &mut CoroutinePool) -> Self {
        CoroutineCreator {
            pool: unsafe { Box::leak(Box::from_raw(pool)) },
        }
    }
}

impl Listener for CoroutineCreator {
    fn on_suspend(&self, _co: &SchedulableCoroutine) {
        _ = self.pool.grow();
    }

    fn on_syscall(&self, _co: &SchedulableCoroutine, _syscall_name: &str) {
        _ = self.pool.grow();
    }
}
