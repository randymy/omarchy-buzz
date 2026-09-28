// Test doubles for the ACP transport and mode type; no adapter is executed.
use std::time::Duration;
const PERMISSION_MODE_TIMEOUT: Duration = Duration::from_millis(15);
struct PermissionMode(&'static str);
impl PermissionMode { fn as_wire_str(&self) -> &str { self.0 } }
struct AcpClient {
    response: Option<Result<serde_json::Value, AcpError>>,
    calls: Vec<(String, String, String)>,
}
impl AcpClient {
    async fn session_set_config_option(&mut self, session: &str, key: &str, value: &str)
        -> Result<serde_json::Value, AcpError> {
        self.calls.push((session.into(), key.into(), value.into()));
        match self.response.take() {
            Some(result) => result,
            None => std::future::pending().await,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn requested_mode_requires_exact_advertisement() {
        for raw in [json!({}), json!({"modes":null}),
            json!({"modes":{"availableModes":[{"id":"default"}]}}),
            json!({"modes":{"availableModes":[{"id":true}]}})] {
            assert!(require_permission_mode(&raw, "plan").is_err());
        }
        assert!(require_permission_mode(&json!({"modes":{"availableModes":[{"id":"plan"}]}}), "plan").is_ok());
    }
    #[tokio::test]
    async fn accepted_mode_uses_exact_session_and_value() {
        let mut client=AcpClient {response:Some(Ok(json!({}))),calls:vec![]};
        apply_permission_mode(&mut client,"synthetic-session",&PermissionMode("plan")).await.unwrap();
        assert_eq!(client.calls,vec![("synthetic-session".into(),"mode".into(),"plan".into())]);
    }
    #[tokio::test]
    async fn application_and_transport_errors_cannot_fall_back() {
        let errors=vec![AcpError::AgentError {code:-32602,message:"synthetic unsupported mode".into()},
            AcpError::AgentExited,AcpError::Protocol("synthetic malformed response".into()),
            AcpError::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
            AcpError::Timeout(Duration::from_millis(1)),AcpError::WriteTimeout(Duration::from_millis(1))];
        for error in errors {
            let expected=std::mem::discriminant(&error);
            let mut client=AcpClient {response:Some(Err(error)),calls:vec![]};
            let result=apply_permission_mode(&mut client,"synthetic-session",&PermissionMode("plan")).await.unwrap_err();
            assert_eq!(std::mem::discriminant(&result),expected);
            assert_eq!(client.calls.len(),1,"must not retry with another mode");
        }
    }
    #[tokio::test]
    async fn unanswered_mode_request_times_out_without_retry() {
        let mut client=AcpClient {response:None,calls:vec![]};
        assert!(matches!(apply_permission_mode(&mut client,"synthetic-session",&PermissionMode("plan")).await,Err(AcpError::Timeout(_))));
        assert_eq!(client.calls.len(),1);
    }
}
