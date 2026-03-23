use std::fmt;
use std::str::FromStr;

use crate::api::Error;

/// An API token for authenticating with the Proxmox API.
///
/// Tokens follow the format: `user@realm!tokenid=secret`
#[derive(Debug, Clone)]
pub struct ApiToken {
    pub user: String,
    pub realm: String,
    pub token_id: String,
    pub secret: String,
}

impl FromStr for ApiToken {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split on '=' to separate secret
        let (token_part, secret) = s.split_once('=').ok_or_else(|| {
            Error::Config(
                "invalid token format: missing '=' separator (expected user@realm!tokenid=secret)"
                    .to_string(),
            )
        })?;

        // Split token_part on '!' to get user@realm and token_id
        let (user_realm, token_id) = token_part.split_once('!').ok_or_else(|| {
            Error::Config(
                "invalid token format: missing '!' separator (expected user@realm!tokenid=secret)"
                    .to_string(),
            )
        })?;

        // Split user_realm on '@' to separate user and realm
        let (user, realm) = user_realm.split_once('@').ok_or_else(|| {
            Error::Config(
                "invalid token format: missing '@' separator (expected user@realm!tokenid=secret)"
                    .to_string(),
            )
        })?;

        if user.is_empty() {
            return Err(Error::Config(
                "invalid token: user must not be empty".to_string(),
            ));
        }
        if realm.is_empty() {
            return Err(Error::Config(
                "invalid token: realm must not be empty".to_string(),
            ));
        }
        if token_id.is_empty() {
            return Err(Error::Config(
                "invalid token: token_id must not be empty".to_string(),
            ));
        }
        if secret.is_empty() {
            return Err(Error::Config(
                "invalid token: secret must not be empty".to_string(),
            ));
        }

        Ok(ApiToken {
            user: user.to_string(),
            realm: realm.to_string(),
            token_id: token_id.to_string(),
            secret: secret.to_string(),
        })
    }
}

impl ApiToken {
    /// Returns the Authorization header value for Proxmox API requests.
    pub fn auth_header(&self) -> String {
        format!(
            "PVEAPIToken={}@{}!{}={}",
            self.user, self.realm, self.token_id, self.secret
        )
    }
}

impl fmt::Display for ApiToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let masked = if self.secret.len() > 8 {
            format!(
                "{}****{}",
                &self.secret[..4],
                &self.secret[self.secret.len() - 4..]
            )
        } else {
            "****".to_string()
        };
        write!(
            f,
            "{}@{}!{}={}",
            self.user, self.realm, self.token_id, masked
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_token() {
        let token: ApiToken = "root@pam!mytoken=abc123def456".parse().unwrap();
        assert_eq!(token.user, "root");
        assert_eq!(token.realm, "pam");
        assert_eq!(token.token_id, "mytoken");
        assert_eq!(token.secret, "abc123def456");
    }

    #[test]
    fn test_parse_complex_user() {
        let token: ApiToken = "admin.user@pve!ci-token=supersecretvalue".parse().unwrap();
        assert_eq!(token.user, "admin.user");
        assert_eq!(token.realm, "pve");
        assert_eq!(token.token_id, "ci-token");
        assert_eq!(token.secret, "supersecretvalue");
    }

    #[test]
    fn test_parse_missing_equals_returns_error() {
        let result = "root@pam!mytoken".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("'='"));
    }

    #[test]
    fn test_parse_missing_exclamation_returns_error() {
        let result = "root@pam=secret".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("'!'"));
    }

    #[test]
    fn test_parse_missing_at_returns_error() {
        let result = "rootpam!mytoken=secret".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("'@'"));
    }

    #[test]
    fn test_parse_empty_user_returns_error() {
        let result = "@pam!mytoken=secret".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("user"));
    }

    #[test]
    fn test_parse_empty_realm_returns_error() {
        let result = "root@!mytoken=secret".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("realm"));
    }

    #[test]
    fn test_parse_empty_token_id_returns_error() {
        let result = "root@pam!=secret".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("token_id"));
    }

    #[test]
    fn test_parse_empty_secret_returns_error() {
        let result = "root@pam!mytoken=".parse::<ApiToken>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        assert!(err.to_string().contains("secret"));
    }

    #[test]
    fn test_auth_header_format() {
        let token: ApiToken = "root@pam!mytoken=abc123".parse().unwrap();
        assert_eq!(token.auth_header(), "PVEAPIToken=root@pam!mytoken=abc123");
    }

    #[test]
    fn test_auth_header_preserves_full_secret() {
        let token: ApiToken = "admin@pve!citoken=supersecretvalue123".parse().unwrap();
        assert_eq!(
            token.auth_header(),
            "PVEAPIToken=admin@pve!citoken=supersecretvalue123"
        );
    }

    #[test]
    fn test_display_masks_long_secret() {
        let token: ApiToken = "root@pam!mytoken=abcdefghijklmnop".parse().unwrap();
        let display = token.to_string();
        // Long secret (>8 chars): show first 4 + last 4 with **** in middle
        assert_eq!(display, "root@pam!mytoken=abcd****mnop");
        assert!(!display.contains("efghijkl"));
    }

    #[test]
    fn test_display_masks_short_secret() {
        let token: ApiToken = "root@pam!mytoken=short".parse().unwrap();
        let display = token.to_string();
        // Short secret (≤8 chars): show ****
        assert_eq!(display, "root@pam!mytoken=****");
        assert!(!display.contains("short"));
    }

    #[test]
    fn test_display_masks_exactly_8_char_secret() {
        let token: ApiToken = "root@pam!mytoken=12345678".parse().unwrap();
        let display = token.to_string();
        // Exactly 8 chars: not > 8, so masked as ****
        assert_eq!(display, "root@pam!mytoken=****");
    }

    #[test]
    fn test_display_masks_9_char_secret() {
        let token: ApiToken = "root@pam!mytoken=123456789".parse().unwrap();
        let display = token.to_string();
        // 9 chars: > 8, so first 4 + last 4
        assert_eq!(display, "root@pam!mytoken=1234****6789");
    }
}
