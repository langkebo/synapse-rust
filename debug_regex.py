import re

content = """
async fn whoami(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
"""

handler = "whoami"
handler_sig_pattern = rf'async\s+fn\s+{re.escape(handler)}\s*\((?P<args>.*?)\)'
h_match = re.search(handler_sig_pattern, content, re.DOTALL)
if h_match:
    args = h_match.group('args')
    print(f"ARGS: '{args}'")
    if "AdminUser" in args: print("Auth: Admin")
    elif "AuthenticatedUser" in args: print("Auth: User")
else:
    print("No match")
