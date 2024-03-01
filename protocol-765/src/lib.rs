// https://wiki.vg/Protocol

// The login process is as follows:
// 1. C→S: Handshake with Next State set to 2 (login)
// 2. C→S: Login Start
// 3. S→C: Encryption Request
// 4. Client auth
// 5. C→S: Encryption Response
// 6. Server auth, both enable encryption
// 7. S→C: Set Compression (optional)
// 8. S→C: Login Success
// 9. C→S: Login Acknowledged

pub mod clientbound;
pub mod serverbound;
pub mod status;
