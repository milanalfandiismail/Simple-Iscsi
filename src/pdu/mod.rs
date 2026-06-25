#![allow(dead_code)]

pub mod builder;
pub mod parser;

// Opcode iSCSI Initiator (Client -> Target)
pub const OP_NOP_OUT: u8 = 0x00;
pub const OP_SCSI_CMD: u8 = 0x01;
pub const OP_LOGIN_REQ: u8 = 0x03;
pub const OP_TEXT_REQ: u8 = 0x04;
pub const OP_DATA_OUT: u8 = 0x05;
pub const OP_LOGOUT_REQ: u8 = 0x06;

// Opcode iSCSI Target (Target -> Client)
pub const OP_NOP_IN: u8 = 0x20;
pub const OP_SCSI_RESP: u8 = 0x21;
pub const OP_LOGIN_RESP: u8 = 0x23;
pub const OP_TEXT_RESP: u8 = 0x24;
pub const OP_DATA_IN: u8 = 0x25;
pub const OP_LOGOUT_RESP: u8 = 0x26;
pub const OP_R2T: u8 = 0x31;

// Konstanta Tahap Login (CSG / NSG)
pub const STAGE_SECURITY_NEGOTIATION: u8 = 0;
pub const STAGE_LOGIN_OPERATIONAL_NEGOTIATION: u8 = 1;
pub const STAGE_FULL_FEATURE_PHASE: u8 = 3;

#[derive(Debug, Clone)]
pub struct Pdu {
    pub opcode: u8,
    pub is_immediate: bool,
    pub flags: u8,
    pub opcode_specific: [u8; 3],
    pub data_segment_len: u32,
    pub lun: u64,
    pub initiator_task_tag: u32,
    pub expected_data_len: u32,
    pub cmd_sn: u32,
    pub exp_stat_sn: u32,
    pub max_cmd_sn: u32,
    pub custom_bhs: [u8; 16], // Misal CDB untuk SCSI Command
    pub data: Vec<u8>,
}

impl Default for Pdu {
    fn default() -> Self {
        Pdu {
            opcode: 0,
            is_immediate: false,
            flags: 0,
            opcode_specific: [0; 3],
            data_segment_len: 0,
            lun: 0,
            initiator_task_tag: 0,
            expected_data_len: 0,
            cmd_sn: 0,
            exp_stat_sn: 0,
            max_cmd_sn: 0,
            custom_bhs: [0; 16],
            data: Vec::new(),
        }
    }
}
