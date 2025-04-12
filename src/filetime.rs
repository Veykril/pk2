//! Windows `FILETIME`

/// Windows `FILETIME`
#[allow(non_snake_case, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

impl Default for FILETIME {
    fn default() -> Self {
        Self { dwLowDateTime: Self::MS_EPOCH as u32, dwHighDateTime: (Self::MS_EPOCH >> 32) as u32 }
    }
}

impl FILETIME {
    const MS_EPOCH: u64 = 11_6444_7360_0000_0000;
}

#[cfg(feature = "std")]
mod std_impls {
    use std::time::SystemTime;

    use crate::FILETIME;

    impl FILETIME {
        const RESOLUTION_SCALE: u128 = 100;

        #[inline]
        pub fn now() -> Self {
            SystemTime::now().into()
        }

        pub fn into_systime(self) -> Option<SystemTime> {
            let FILETIME { dwLowDateTime: low, dwHighDateTime: high } = self;
            let ftime = ((high as u64) << 32) | low as u64;
            let nanos =
                ftime.checked_sub(FILETIME::MS_EPOCH)? * (FILETIME::RESOLUTION_SCALE as u64);
            Some(SystemTime::UNIX_EPOCH + core::time::Duration::from_nanos(nanos))
        }
    }

    impl From<SystemTime> for FILETIME {
        fn from(time: SystemTime) -> Self {
            let duration = time.duration_since(SystemTime::UNIX_EPOCH).unwrap();
            let ftime = (duration.as_nanos() / Self::RESOLUTION_SCALE) as u64 + Self::MS_EPOCH;
            FILETIME { dwLowDateTime: ftime as u32, dwHighDateTime: (ftime >> 32) as u32 }
        }
    }

    #[test]
    fn test_convert_roundtrip() {
        use core::time::Duration;

        let now = SystemTime::now();
        let ftime = FILETIME::from(now);
        let round_trip = ftime.into_systime().unwrap();
        // we compare it this way because the conversion loses precision due to the
        // division by 100. So comparing it by dividing the nanoseconds we will get a
        // lossless comparison for systems that have a higher timer resolution.
        assert_eq!(
            now.duration_since(SystemTime::UNIX_EPOCH)
                .as_ref()
                .map(Duration::as_nanos)
                .map(|nanos| nanos / FILETIME::RESOLUTION_SCALE)
                .unwrap(),
            round_trip
                .duration_since(SystemTime::UNIX_EPOCH)
                .as_ref()
                .map(Duration::as_nanos)
                .map(|nanos| nanos / FILETIME::RESOLUTION_SCALE)
                .unwrap()
        );
    }
}
