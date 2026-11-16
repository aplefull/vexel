use super::{BlockTables, FastExit, Inflater};

impl Inflater<'_> {
    pub(super) fn run_fast_loop(&mut self, tables: &BlockTables, out_end: usize) -> FastExit {
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("bmi1") && is_x86_feature_detected!("bmi2") {
            return unsafe {
                if tables.paired {
                    self.fast_loop_bmi2::<true>(tables, out_end)
                } else {
                    self.fast_loop_bmi2::<false>(tables, out_end)
                }
            };
        }

        if tables.paired {
            self.fast_loop::<true>(tables, out_end)
        } else {
            self.fast_loop::<false>(tables, out_end)
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "bmi1,bmi2")]
    unsafe fn fast_loop_bmi2<const PAIRED: bool>(&mut self, tables: &BlockTables, out_end: usize) -> FastExit {
        self.fast_loop::<PAIRED>(tables, out_end)
    }
}
