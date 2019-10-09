trait EngineDriver {
    fn poll(&mut self) -> Vec<Event>;
}
