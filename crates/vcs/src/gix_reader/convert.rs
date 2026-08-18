use domain::{RepositoryError, Signature, Timestamp};

use super::error::backend;

/// Converts a `gix` object id into the domain's SHA-1-only [`domain::ObjectId`].
///
/// [`crate::gix_reader::GixRepositoryReader::open`] rejects, at open time, any repository
/// whose object format is not SHA-1, and this build of `gix-hash` has the `sha256` feature
/// disabled, so [`gix::ObjectId`] cannot represent anything but a twenty-byte SHA-1 hash.
pub(crate) fn to_domain_id(id: gix::ObjectId) -> domain::ObjectId {
    let bytes: [u8; 20] = id
        .as_slice()
        .try_into()
        .expect("gix::ObjectId is always twenty bytes: sha256 is disabled in this build");
    domain::ObjectId::from_bytes(bytes)
}

pub(crate) fn to_gix_id(id: domain::ObjectId) -> gix::ObjectId {
    gix::ObjectId::from(*id.as_bytes())
}

pub(crate) fn to_signature(
    signature: gix::actor::SignatureRef<'_>,
) -> Result<Signature, RepositoryError> {
    let time = signature
        .time()
        .map_err(|err| backend("parsing commit signature time", err))?;
    let offset_minutes = i16::try_from(time.offset / 60)
        .map_err(|err| backend("commit signature offset out of range", err))?;
    Ok(Signature {
        name: signature.name.to_string(),
        email: signature.email.to_string(),
        time: Timestamp {
            seconds: time.seconds,
            offset_minutes,
        },
    })
}
