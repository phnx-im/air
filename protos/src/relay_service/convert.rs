use aircommon::identifiers;

use crate::relay_service::v1::QsUserId;

impl From<identifiers::QsUserId> for QsUserId {
    fn from(value: identifiers::QsUserId) -> Self {
        let uuid = *value.as_uuid();
        Self {
            value: Some(uuid.into()),
        }
    }
}
