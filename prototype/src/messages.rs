use crate::{IdMap, PartitionId};

pub enum Message {
    Test,
}

pub struct MessageSender {
    for_partition: IdMap<flume::Sender<Message>>,
    for_global: flume::Sender<Message>,
}

impl MessageSender {
    pub fn send_partition(&self, id: PartitionId, message: Message) -> anyhow::Result<()> {
        let Some(partition) = self.for_partition.get(id) else {
            panic!("somehow tried to send a message to a partition that doesn't exist");
        };

        partition.send(message)?;
        Ok(())
    }

    pub fn send_global(&self, message: Message) -> anyhow::Result<()> {
        self.for_global.send(message)?;
        Ok(())
    }
}

pub struct MessageReceiver {
    for_partition: IdMap<flume::Receiver<Message>>,
    for_global: flume::Receiver<Message>,
}

impl MessageReceiver {
    pub fn recv_partition(&self, id: PartitionId) -> anyhow::Result<Message> {
        let Some(partition) = self.for_partition.get(id) else {
            panic!("somehow tried to receive a message from a partition that doesn't exist");
        };

        let v = partition.recv()?;
        Ok(v)
    }

    pub fn recv_global(&self) -> anyhow::Result<Message> {
        let v = self.for_global.recv()?;
        Ok(v)
    }
}
