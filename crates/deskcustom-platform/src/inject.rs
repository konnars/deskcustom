use crate::{InputEvent, InputInject};

pub struct NoopInject;

impl InputInject for NoopInject {
    fn inject(&mut self, _event: &InputEvent) -> anyhow::Result<()> {
        Ok(())
    }
}
