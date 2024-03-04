// TODO: Add custom profiling similar to puffin
// pub static mut EVENT_LOG: EventLog = EventLog::new();
// static EVENT_LOG_INDEX: AtomicU64 = AtomicU64::new(0);
// static TIMER_FREQ: Lazy<f64> = Lazy::new(|| estimated_cpu_timer_freq() as f64);

pub static mut EVENT_LOG: Vec<(&'static str, u64, u64)> = Vec::new();
// pub static mut LAST_PRINT: std::time::SystemTime = std::time::UNIX_EPOCH;

pub fn start(_name: &'static str) -> u64 {
    // #[cfg(feature = "profiling")]
    // unsafe {
    //     std::arch::x86_64::_rdtsc()
    // }
    // #[cfg(not(feature = "profiling"))]
    0
}

pub fn end(_name: &'static str, _tsc: u64) {
    // #[cfg(feature = "profiling")]
    // unsafe {
    //     let now = std::arch::x86_64::_rdtsc();
    //     if let Some((_, d, c)) = EVENT_LOG.iter_mut().find(|(n, ..)| *n == name) {
    //         *d += now - tsc;
    //         *c += 1;
    //     } else {
    //         EVENT_LOG.push((name, now - tsc, 1));
    //     }
    // }
}

pub fn end_frame() {
    // #[cfg(feature = "profiling")]
    // unsafe {
    //     if std::time::SystemTime::now()
    //         .duration_since(LAST_PRINT)
    //         .unwrap()
    //         > std::time::Duration::from_secs(1)
    //     {
    //         for (name, tsc, count) in EVENT_LOG.iter() {
    //             println!("{name}: {}", tsc / *count);
    //         }
    //         println!("---");
    //         EVENT_LOG.clear();
    //         LAST_PRINT = std::time::SystemTime::now();
    //     }
    // }
}

pub fn init() {
    #[cfg(feature = "profiling")]
    enable(true);
    // unsafe {
    //     EVENT_LOG.reserve(2 << 20);
    //     LAST_PRINT = std::time::SystemTime::now();
    // }
}

#[cfg(feature = "profiling")]
pub fn enable(enabled: bool) {
    puffin::set_scopes_on(enabled);
}

/// Begin profiling an arbitrary range of lines not inside a block, tagging it with a given name. For
/// profiling a block of code, see [`profile!`].
#[macro_export]
macro_rules! profile_start {
    ($name:expr) => {
        // #[cfg(feature = "profiling")]
        // $crate::profiling::record_event(
        //     $crate::profiling::EventType::BeginBlock,
        //     $name,
        //     file!(),
        //     module_path!(),
        //     line!(),
        // );
    };
}

/// End profiling an arbitrary range of lines not inside a block started with [`profile_start!`]
/// with a given name. For profiling a block of code, see [`profile!`].
#[macro_export]
macro_rules! profile_end {
    ($name:expr) => {
        // #[cfg(feature = "profiling")]
        // $crate::profiling::record_event(
        //     $crate::profiling::EventType::EndBlock,
        //     $name,
        //     file!(),
        //     module_path!(),
        //     line!(),
        // );
    };
}

/// Profile a given function or block of code. This macro will automatically use the fully
/// qualified function name when used without arguments. You can also optionally pass a custom name
/// for a given block scope.
///
/// For profiling an arbitrary range of lines not inside a block, see [`profile_start!`] and
/// [`profile_end!`].
///
/// # Examples
///
/// ```
/// use util_lib_rs::profile;
///
/// fn my_function() {
///     profile!();
///
///     for _ in 0..10000 {
///         profile!("loop");
///     }
/// }
/// ```
#[macro_export]
macro_rules! profile {
    () => {
        $crate::profile!("");
    };
    ($data:expr) => {
        #[cfg(feature = "profiling")]
        puffin::profile_function!($data);
        // const fn __f() {}
        // profile!($crate::profiling::function_name(__f));
    };
    ($name:expr) => {
        $crate::profile!($name, "");
    };
    ($name:expr, $data:expr) => {
        #[cfg(feature = "profiling")]
        puffin::profile_scope!($name, $data);
        // TODO: fix possible name collision
        // #[cfg(feature = "profiling")]
        // let __block = $crate::profiling::TimedBlock::new($name, file!(), module_path!(), line!());
    };
}

/// Begin a profiling frame.
#[macro_export]
macro_rules! frame_begin {
    () => {
        #[cfg(feature = "profiling")]
        puffin::GlobalProfiler::lock().new_frame();
        // TODO: Add seconds elapsed since game start
        // #[cfg(feature = "profiling")]
        // $crate::profiling::record_event(
        //     $crate::profiling::EventType::FrameEnd,
        //     "Frame End",
        //     file!(),
        //     module_path!(),
        //     line!(),
        // );
        // unsafe { $crate::profiling::EVENT_LOG.frame_end() };
    };
}

// #[derive(Debug)]
// #[must_use]
// struct EventFrame {
//     start_clock: u64,
//     end_clock: Option<u64>,
//     seconds_elapsed: Option<f64>,
// }

// impl EventFrame {
//     const fn new(start_clock: u64) -> Self {
//         Self {
//             start_clock,
//             end_clock: None,
//             seconds_elapsed: None,
//         }
//     }
// }

// #[derive(Debug)]
// #[must_use]
// pub struct EventLog {
//     current_event_frame_index: usize,
//     event_frames: [[Option<Event>; Self::EVENTS_SIZE]; Self::EVENT_FRAME_SIZE],
// }

// impl EventLog {
//     const EVENT_FRAME_SIZE: usize = 8;
//     const EVENTS_SIZE: usize = 65536;

//     const fn new() -> Self {
//         Self {
//             current_event_frame_index: 0,
//             event_frames: [[None; Self::EVENTS_SIZE]; Self::EVENT_FRAME_SIZE],
//         }
//     }

//     pub fn frame_end(&mut self) {
//         self.current_event_frame_index += 1;
//         if self.current_event_frame_index >= Self::EVENT_FRAME_SIZE {
//             self.current_event_frame_index = 0;
//         }
//         EVENT_LOG_INDEX
//             .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| {
//                 Some((self.current_event_frame_index as u64) << 32)
//             })
//             .expect("should never fail");
//         self.collate_events(self.current_event_frame_index);
//     }

//     fn collate_events(&self, invalid_event_frame_index: usize) {
//         let mut event_frame_index = invalid_event_frame_index + 1;
//         let mut current_frame: Option<EventFrame> = None;
//         let mut event_thread_blocks = HashMap::<ThreadId, Vec<Event>>::new();
//         loop {
//             match event_frame_index {
//                 Self::EVENT_FRAME_SIZE => {
//                     event_frame_index = 0;
//                     continue;
//                 }
//                 i if i == invalid_event_frame_index => break,
//                 event_frame_index => {
//                     for event in self.event_frames[event_frame_index]
//                         .iter()
//                         .take_while(|e| e.is_some())
//                         .filter_map(|e| *e)
//                     {
//                         match event.ty {
//                             EventType::FrameEnd => {
//                                 if let Some(mut current_frame) = current_frame {
//                                     current_frame.end_clock = Some(event.clock);
//                                     current_frame.seconds_elapsed = Some(event.seconds_elapsed);
//                                     let clocks_elapsed =
//                                         event.clock.saturating_sub(current_frame.start_clock);
//                                     if clocks_elapsed > 0 {
//                                         let milliseconds_elapsed =
//                                             1000.0 * clocks_elapsed as f64 / *TIMER_FREQ;
//                                         println!(
//                                             "Frame: {milliseconds_elapsed}ms, {clocks_elapsed} clocks",
//                                         );
//                                     }
//                                 }
//                                 current_frame = Some(EventFrame::new(event.clock));
//                             }
//                             EventType::BeginBlock => {
//                                 match event_thread_blocks.entry(event.thread_id) {
//                                     Entry::Occupied(mut entry) => {
//                                         entry.get_mut().push(event);
//                                     }
//                                     Entry::Vacant(entry) => {
//                                         let mut events = Vec::with_capacity(Self::EVENTS_SIZE);
//                                         events.push(event);
//                                         entry.insert(events);
//                                     }
//                                 }
//                             }
//                             EventType::EndBlock => {
//                                 match event_thread_blocks.entry(event.thread_id) {
//                                     Entry::Occupied(mut entry) => {
//                                         let events = entry.get_mut();
//                                         if let Some(begin_event) = events.last().copied() {
//                                             events.pop();
//                                             if begin_event.clock > event.clock {
//                                                 eprintln!(
//                                                     "end block event before begin block event"
//                                                 );
//                                             }

//                                             let clocks_elapsed =
//                                                 event.clock.saturating_sub(begin_event.clock);
//                                             if clocks_elapsed > 0 {
//                                                 let milliseconds_elapsed =
//                                                     1000.0 * clocks_elapsed as f64 / *TIMER_FREQ;
//                                                 println!(
//                                                     "Block: {} {milliseconds_elapsed}ms, {clocks_elapsed} clocks",
//                                                     event.name
//                                                 );
//                                             }
//                                         } else {
//                                             eprintln!("end block event without begin block event");
//                                         }
//                                     }
//                                     Entry::Vacant(_) => {
//                                         eprintln!("end block event without begin block event");
//                                     }
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }

//             event_frame_index += 1;
//         }
//     }
// }

// #[derive(Debug, Copy, Clone)]
// #[must_use]
// pub enum EventType {
//     FrameEnd,
//     BeginBlock,
//     EndBlock,
// }

// #[derive(Debug, Copy, Clone)]
// #[must_use]
// struct Event {
//     clock: u64,
//     ty: EventType,
//     thread_id: ThreadId,
//     name: &'static str,
//     file: &'static str,
//     module_path: &'static str,
//     line: u32,
//     seconds_elapsed: f64,
// }

// impl Event {
//     fn new(
//         ty: EventType,
//         name: &'static str,
//         file: &'static str,
//         module_path: &'static str,
//         line: u32,
//     ) -> Self {
//         Self {
//             clock: read_tsc(),
//             ty,
//             thread_id: thread::current().id(),
//             name,
//             file,
//             module_path,
//             line,
//             seconds_elapsed: 0.0,
//         }
//     }
// }

// #[derive(Debug)]
// #[must_use]
// pub struct TimedBlock(&'static str);

// impl TimedBlock {
//     pub fn new(
//         name: &'static str,
//         file: &'static str,
//         module_path: &'static str,
//         line: u32,
//     ) -> Self {
//         record_event(EventType::BeginBlock, name, file, module_path, line);
//         Self(name)
//     }
// }

// impl Drop for TimedBlock {
//     fn drop(&mut self) {
//         record_event(
//             EventType::EndBlock,
//             self.0,
//             file!(),
//             module_path!(),
//             line!(),
//         );
//     }
// }

// pub fn record_event(
//     ty: EventType,
//     name: &'static str,
//     file: &'static str,
//     module_path: &'static str,
//     line: u32,
// ) {
//     let event_frame_event_index = EVENT_LOG_INDEX.fetch_add(1, Ordering::Relaxed);
//     let event_frame_index = (event_frame_event_index >> 32) as usize;
//     let event_index = (event_frame_event_index & 0xFFFF_FFFF) as usize;
//     assert!(event_index < EventLog::EVENTS_SIZE);
//     unsafe {
//         EVENT_LOG.event_frames[event_frame_index][event_index] =
//             Some(Event::new(ty, name, file, module_path, line));
//     }
// }

// fn read_tsc() -> u64 {
//     #[cfg(target_arch = "x86")]
//     unsafe {
//         let mut aux = 0;
//         std::arch::x86::__rdtscp(&mut aux)
//     }
//     #[cfg(target_arch = "x86_64")]
//     unsafe {
//         let mut aux = 0;
//         std::arch::x86_64::__rdtscp(&mut aux)
//     }
//     #[cfg(target_os = "macos")]
//     {
//         let mut tsc;
//         unsafe { std::arch::asm!("mrs {tsc}, cntvct_el0", tsc = out(reg) tsc) }
//         tsc
//     }
//     #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_os = "macos")))]
//     panic!("performance profiling is not supported on this architecture")
// }

// #[must_use]
// pub fn function_name<T>(_: T) -> &'static str {
//     let name = std::any::type_name::<T>();
//     name[..name.len().saturating_sub(5)].into()
// }

// /// Returns a conversion factor for OS timer. In the case of linux, the units are in microseconds.
// fn get_os_freq() -> u64 {
//     #[cfg(all(target_os = "unix", not(target_os = "macos")))]
//     {
//         1_000_000
//     }

//     #[cfg(target_os = "macos")]
//     {
//         let mut freq;
//         unsafe { std::arch::asm!("mrs {freq}, cntfrq_el0", freq = out(reg) freq) }
//         freq
//     }

//     #[cfg(not(any(target_os = "unix", target_os = "macos")))]
//     panic!("performance profiling is not supported on this architecture")
// }

// fn read_os_timer() -> u64 {
//     use std::time::{SystemTime, UNIX_EPOCH};
//     let since_epoch = SystemTime::now()
//         .duration_since(UNIX_EPOCH)
//         .expect("system time is earlier than Unix Epoch");
//     get_os_freq() * since_epoch.as_secs() + u64::from(since_epoch.subsec_micros())
// }

// fn estimated_cpu_timer_freq() -> u64 {
//     let milliseconds_to_wait = 1000;
//     let os_freq = get_os_freq();

//     let tsc_start = read_tsc();
//     let os_start = read_os_timer();
//     let mut os_end;
//     let mut os_elapsed = 0;
//     let os_wait_time = os_freq * milliseconds_to_wait / 1000;
//     while os_elapsed < os_wait_time {
//         os_end = read_os_timer();
//         os_elapsed = os_end - os_start;
//     }

//     let tsc_end = read_tsc();
//     let tsc_elapsed = tsc_end - tsc_start;

//     if os_elapsed > 0 {
//         os_freq * tsc_elapsed / os_elapsed
//     } else {
//         0
//     }
// }
