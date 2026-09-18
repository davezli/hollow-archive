//! Game-derived constants. Kept in one place because a client update can rotate them.
//! Values copied from zzz_packet_capture `src/crypto/key.hpp` (game 3.2).

/// RSA-1024 "client" private key the game uses to decrypt `server_rand_key`
/// in `PlayerGetTokenScRsp`. .NET `RSAKeyValue` XML components, base64.
pub const RSA_MODULUS_B64: &str = "rkQoCtGS5YSrzxm89Wq3GSR/uw5AJDwGqu+tkXViZwOF8H6xgL7KPi2OVATHCoaNFLTAD5nlLSjg0pHEAqHafvXtzj4Gh8tvF2A6/8yB5ceT3Oszo9UR5d7hsI55sxb37QUpQLHYoxKs79FohJ74Z5V2LgSY+0XvbGxMHxhxHTc=";
pub const RSA_EXPONENT_B64: &str = "AQAB";
pub const RSA_D_B64: &str = "dBCe3r3Aaa9YQsIwsP/XXR6LGAmgvMFh631gi62z0UpubcPj8wyfZJQw5FKeQqtk0XKlLH7iPZapTnWZJ+umuqf84RcuilwItH3yTrJw8PeIIxuKE8E+TknKni2I7SYyKpuuRbW96DC5WhHS4QNRbNwoyCQPfK0WobQItAWuWFk=";
pub const RSA_P_B64: &str = "59AY/wNEm513fJDXqnnz5D8VrwLnAAPPhiKAy+RsjPh1MDFt2clTQOdL7z1RWwh1fFF819bIUixDJZXie3l9zQ==";
pub const RSA_Q_B64: &str = "wHLxcB3N2s/GJgco35bEJPQkrv184PXnJRO2cTxCR8ZTrN62oKZbzD/6j5EcZA6hUou+YjhPLsqcaFJVI2DjEw==";

/// RSA-1024 block size in bytes.
pub const RSA_KEY_SIZE: usize = 128;

/// Seconds between 0001-01-01 (.NET ticks epoch) and 1970-01-01.
pub const DOTNET_EPOCH_OFFSET_SECS: i64 = 62_135_596_800;

/// How far (in seconds, either side) from the capture timestamp to search for
/// the client's time-seeded random key.
pub const BRUTE_FORCE_WINDOW_SECS: i64 = 5;
