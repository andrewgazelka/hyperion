use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Players {
    pub online: i64,
    pub max: i64,
    pub sample: Vec<Sample>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub name: String,
    pub protocol: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub version: Version,
    pub players: Players,
    pub description: Description,
    // pub favicon: String,
    // pub enforces_secure_chat: bool,
    // pub previews_chat: bool,
}

impl Root {
    #[must_use]
    pub fn sample() -> Self {
        // {
        //     "version": {
        //         "name": "1.19.4",
        //         "protocol": 762
        //     },
        //     "players": {
        //         "max": 100,
        //         "online": 5,
        //         "sample": [
        //             {
        //                 "name": "thinkofdeath",
        //                 "id": "4566e69f-c907-48ee-8d71-d7ba5aa00d20"
        //             }
        //         ]
        //     },
        //     "description": {
        //         "text": "Hello world"
        //     },
        //     "favicon": "data:image/png;base64,<data>",
        //     "enforcesSecureChat": true,
        //     "previewsChat": true
        // }

        Self {
            version: Version {
                name: "1.20.4".to_string(),
                protocol: 765,
            },
            players: Players {
                max: 100,
                online: 0,
                sample: vec![],
            },
            description: Description {
                text: "Hello world".to_string(),
            },
            // favicon: "data:image/png;base64,<data>".to_string(),
            // enforces_secure_chat: true,
            // previews_chat: true,
        }
    }
}
