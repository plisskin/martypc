/*
    MartyPC
    https://github.com/dbalsom/martypc

    Copyright 2022-2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    ---------------------------------------------------------------------------

    cpu_808x::logging.rs

    Implements cycle-state logging facilities.

*/

use crate::{
    cpu_808x::{
        microcode::{MC_CORR, MC_JUMP, MC_NONE, MC_RTN, MICROCODE_NUL, MICROCODE_SRC_8088},
        BiuStateNew,
        BusStatus,
        Cpu,
        DmaState,
        QueueOp,
        Segment,
        TCycle,
    },
    syntax_token::SyntaxToken,
};

impl Cpu {
    pub fn trace_csv_line(&mut self) {
        let q = self.last_queue_op as u8;
        let s = self.bus_status as u8;

        let mut vs = 0;
        let mut hs = 0;
        let mut den = 0;
        let mut brd = 0;
        if let Some(video) = self.bus().primary_video() {
            let (vs_b, hs_b, den_b, brd_b) = video.get_sync();
            vs = if vs_b { 1 } else { 0 };
            hs = if hs_b { 1 } else { 0 };
            den = if den_b { 1 } else { 0 };
            brd = if brd_b { 1 } else { 0 };
        }

        // Segment status bits are valid after ALE.
        if !self.i8288.ale {
            let seg_n = match self.bus_segment {
                Segment::ES => 0,
                Segment::SS => 1,
                Segment::CS | Segment::None => 2,
                Segment::DS => 3,
            };
            self.address_bus = (self.address_bus & 0b1100_1111_1111_1111_1111) | (seg_n << 16);
        }

        // "Time(s),addr,clk,ready,qs,s,clk0,intr,dr0,vs,hs"
        // sigrok import string:
        // t,x20,l,l,x2,x3,l,l,l,l,l,l
        self.trace_emit(&format!(
            "{},{:05X},1,{},{},{},{},{},{},{},{},{},{}",
            self.t_stamp,
            self.address_bus,
            if self.ready { 1 } else { 0 },
            q,
            s,
            0,
            if self.intr { 1 } else { 0 },
            if matches!(self.dma_state, DmaState::Dreq) { 1 } else { 0 },
            vs,
            hs,
            den,
            brd
        ));

        self.trace_emit(&format!(
            "{},{:05X},0,{},{},{},{},{},{},{},{},{},{}",
            self.t_stamp + self.t_step_h,
            self.address_bus,
            if self.ready { 1 } else { 0 },
            q,
            s,
            0,
            if self.intr { 1 } else { 0 },
            if matches!(self.dma_state, DmaState::Dreq) { 1 } else { 0 },
            vs,
            hs,
            den,
            brd
        ));
    }

    pub fn cycle_state_string(&self, dma_count: u16, short: bool) -> String {
        let ale_str = match self.i8288.ale {
            true => "A:",
            false => "  ",
        };

        let mut seg_str = "  ";
        if self.t_cycle != TCycle::T1 {
            // Segment status only valid in T2+
            seg_str = match self.bus_segment {
                Segment::None => "  ",
                Segment::SS => "SS",
                Segment::ES => "ES",
                Segment::CS => "CS",
                Segment::DS => "DS",
            };
        }

        let q_op_chr = match self.last_queue_op {
            QueueOp::Idle => ' ',
            QueueOp::First => 'F',
            QueueOp::Flush => 'E',
            QueueOp::Subsequent => 'S',
        };

        let q_preload_char = match self.queue.has_preload() {
            true => '*',
            false => ' ',
        };

        let biu_state_new_str = match self.biu_state_new {
            BiuStateNew::ToIdle(_) => ">I ",
            BiuStateNew::ToPrefetch(_) => ">PF",
            BiuStateNew::ToEu(_) => ">EU",
            BiuStateNew::Idle => "I  ",
            BiuStateNew::Prefetch => "PF ",
            BiuStateNew::Eu => "EU ",
        };

        /*
        let mut f_op_chr = match self.fetch_state {
            FetchState::Scheduled(_) => 'S',
            FetchState::Aborted(_) => 'A',
            //FetchState::Suspended => '!',
            _ => ' '
        };

        if self.fetch_suspended {
            f_op_chr = '!'
        }
        */

        // All read/write signals are active/low
        let rs_chr = match self.i8288.mrdc {
            true => 'R',
            false => '.',
        };
        let aws_chr = match self.i8288.amwc {
            true => 'A',
            false => '.',
        };
        let ws_chr = match self.i8288.mwtc {
            true => 'W',
            false => '.',
        };
        let ior_chr = match self.i8288.iorc {
            true => 'R',
            false => '.',
        };
        let aiow_chr = match self.i8288.aiowc {
            true => 'A',
            false => '.',
        };
        let iow_chr = match self.i8288.iowc {
            true => 'W',
            false => '.',
        };

        let bus_str = match self.bus_status_latch {
            BusStatus::InterruptAck => "IRQA",
            BusStatus::IoRead => "IOR ",
            BusStatus::IoWrite => "IOW ",
            BusStatus::Halt => "HALT",
            BusStatus::CodeFetch => "CODE",
            BusStatus::MemRead => "MEMR",
            BusStatus::MemWrite => "MEMW",
            BusStatus::Passive => "PASV",
        };

        let t_str = match self.t_cycle {
            TCycle::Tinit => "Tx",
            TCycle::Ti => "Ti",
            TCycle::T1 => "T1",
            TCycle::T2 => "T2",
            TCycle::T3 => "T3",
            TCycle::T4 => "T4",
            TCycle::Tw => "Tw",
        };

        let is_reading = self.i8288.mrdc | self.i8288.iorc;
        let is_writing = self.i8288.mwtc | self.i8288.iowc;

        let mut xfer_str = "      ".to_string();
        if is_reading {
            xfer_str = format!("<-r {:02X}", self.data_bus);
        }
        else if is_writing {
            xfer_str = format!("w-> {:02X}", self.data_bus);
        }

        // Handle queue activity

        let mut q_read_str = "      ".to_string();

        let mut instr_str = String::new();

        if self.last_queue_op == QueueOp::First || self.last_queue_op == QueueOp::Subsequent {
            // Queue byte was read.
            q_read_str = format!("<-q {:02X}", self.last_queue_byte);
        }

        if self.last_queue_op == QueueOp::First {
            // First byte of opcode read from queue. Decode the full instruction
            instr_str = format!("[{:04X}:{:04X}] {} ({}) ", self.cs, self.ip, self.i, self.i.size);
        }

        //let mut microcode_str = "   ".to_string();
        let microcode_line_str = match self.trace_instr {
            MC_JUMP => "JMP".to_string(),
            MC_RTN => "RET".to_string(),
            MC_CORR => "COR".to_string(),
            MC_NONE => "   ".to_string(),
            _ => {
                format!("{:03X}", self.trace_instr)
            }
        };

        let microcode_op_str = match self.trace_instr {
            i if usize::from(i) < MICROCODE_SRC_8088.len() => MICROCODE_SRC_8088[i as usize].to_string(),
            _ => MICROCODE_NUL.to_string(),
        };

        let _dma_dreq_chr = match self.dma_aen {
            true => 'R',
            false => '.',
        };

        let tx_cycle = match self.is_last_wait() {
            true => 'x',
            false => '.',
        };

        let ready_chr = if self.wait_states > 0 { '.' } else { 'R' };

        let dma_count_str = &format!("{:02} {:02}", dma_count, self.dram_refresh_cycle_num);

        let dma_str = match self.dma_state {
            DmaState::Idle => dma_count_str,
            DmaState::TimerTrigger => "TIMR",
            DmaState::Dreq => "DREQ",
            DmaState::Hrq => "HRQ ",
            DmaState::HoldA => "HLDA",
            DmaState::Operating(n) => match n {
                4 => "S1",
                3 => "S2",
                2 => "S3",
                1 => "S4",
                _ => "S?",
            }, //DmaState::DmaWait(..) => "DMAW"
        };

        let mut cycle_str;

        if short {
            cycle_str = format!(
                "{:04} {:02}[{:05X}] {:02} {}{} M:{}{}{} I:{}{}{} |{:5}| {:04} {:02} {:06} | {:4}| {:<14}| {:1}{:1}{:1}[{:08}] {} | {:03} | {}",
                self.instr_cycle,
                ale_str,
                self.address_bus,
                seg_str,
                ready_chr,
                self.wait_states,
                rs_chr, aws_chr, ws_chr, ior_chr, aiow_chr, iow_chr,
                dma_str,
                bus_str,
                t_str,
                xfer_str,
                biu_state_new_str,
                format!("{:?}", self.fetch_state),
                q_op_chr,
                self.last_queue_len,
                q_preload_char,
                self.queue.to_string(),
                q_read_str,
                microcode_line_str,
                instr_str
            );
        }
        else {
            cycle_str = format!(
                "{:08}:{:04} {:02}[{:05X}] {:02} {}{}{} M:{}{}{} I:{}{}{} |{:5}|  | {:04} {:02} {:06} | {:4}| {:<14}| {:1}{:1}{:1}[{:08}] {} | {}: {} | {}",
                self.cycle_num,
                self.instr_cycle,
                ale_str,
                self.address_bus,
                seg_str,
                ready_chr,
                self.wait_states,
                tx_cycle,
                rs_chr, aws_chr, ws_chr, ior_chr, aiow_chr, iow_chr,
                dma_str,
                bus_str,
                t_str,
                xfer_str,
                biu_state_new_str,
                format!("{:?}", self.fetch_state),
                q_op_chr,
                self.last_queue_len,
                q_preload_char,
                self.queue.to_string(),
                q_read_str,
                microcode_line_str,
                microcode_op_str,
                instr_str
            );
        }

        for c in &self.trace_comment {
            cycle_str.push_str(&format!("; {}", c));
        }

        cycle_str
    }

    pub fn cycle_state_tokens(&self, dma_count: u16, short: bool) -> Vec<SyntaxToken> {
        let ale_str = match self.i8288.ale {
            true => "A",
            false => " ",
        }
        .to_string();
        let ale_token = SyntaxToken::Text(ale_str);

        let mut seg_str = "  ";
        if self.t_cycle != TCycle::T1 {
            // Segment status only valid in T2+
            seg_str = match self.bus_segment {
                Segment::None => "  ",
                Segment::SS => "SS",
                Segment::ES => "ES",
                Segment::CS => "CS",
                Segment::DS => "DS",
            };
        }
        let seg_token = SyntaxToken::Text(seg_str.to_string());

        let q_op_chr = match self.last_queue_op {
            QueueOp::Idle => ' ',
            QueueOp::First => 'F',
            QueueOp::Flush => 'E',
            QueueOp::Subsequent => 'S',
        };
        let q_op_token = SyntaxToken::Text(q_op_chr.to_string());

        let q_preload_char = match self.queue.has_preload() {
            true => '*',
            false => ' ',
        };

        let biu_state_new_str = match self.biu_state_new {
            BiuStateNew::ToIdle(_) => ">I ",
            BiuStateNew::ToPrefetch(_) => ">PF",
            BiuStateNew::ToEu(_) => ">EU",
            BiuStateNew::Idle => "I  ",
            BiuStateNew::Prefetch => "PF ",
            BiuStateNew::Eu => "EU ",
        };
        let biu_state_new_token = SyntaxToken::Text(biu_state_new_str.to_string());

        /*
        let mut f_op_chr = match self.fetch_state {
            FetchState::Scheduled(_) => 'S',
            FetchState::Aborted(_) => 'A',
            //FetchState::Suspended => '!',
            _ => ' '
        };

        if self.fetch_suspended {
            f_op_chr = '!'
        }
        */

        // All read/write signals are active/low
        let rs_chr = match self.i8288.mrdc {
            true => 'R',
            false => '.',
        };
        let aws_chr = match self.i8288.amwc {
            true => 'A',
            false => '.',
        };
        let ws_chr = match self.i8288.mwtc {
            true => 'W',
            false => '.',
        };
        let ior_chr = match self.i8288.iorc {
            true => 'R',
            false => '.',
        };
        let aiow_chr = match self.i8288.aiowc {
            true => 'A',
            false => '.',
        };
        let iow_chr = match self.i8288.iowc {
            true => 'W',
            false => '.',
        };

        let bus_str = match self.bus_status_latch {
            BusStatus::InterruptAck => "IRQA",
            BusStatus::IoRead => "IOR ",
            BusStatus::IoWrite => "IOW ",
            BusStatus::Halt => "HALT",
            BusStatus::CodeFetch => "CODE",
            BusStatus::MemRead => "MEMR",
            BusStatus::MemWrite => "MEMW",
            BusStatus::Passive => "PASV",
        };
        let bus_str_token = SyntaxToken::Text(bus_str.to_string());

        let t_str = match self.t_cycle {
            TCycle::Tinit => "Tx",
            TCycle::Ti => "Ti",
            TCycle::T1 => "T1",
            TCycle::T2 => "T2",
            TCycle::T3 => "T3",
            TCycle::T4 => "T4",
            TCycle::Tw => "Tw",
        };
        let t_str_token = SyntaxToken::Text(t_str.to_string());

        let is_reading = self.i8288.mrdc | self.i8288.iorc;
        let is_writing = self.i8288.mwtc | self.i8288.iowc;

        let mut xfer_str = "      ".to_string();
        if is_reading {
            xfer_str = format!("<-r {:02X}", self.data_bus);
        }
        else if is_writing {
            xfer_str = format!("w-> {:02X}", self.data_bus);
        }

        // Handle queue activity

        let mut q_read_str = "      ".to_string();

        let mut instr_str = String::new();

        if self.last_queue_op == QueueOp::First || self.last_queue_op == QueueOp::Subsequent {
            // Queue byte was read.
            q_read_str = format!("<-q {:02X}", self.last_queue_byte);
        }
        let q_read_token = SyntaxToken::Text(q_read_str.to_string());

        if self.last_queue_op == QueueOp::First {
            // First byte of opcode read from queue. Decode the full instruction
            instr_str = format!("[{:04X}:{:04X}] {} ({}) ", self.cs, self.ip, self.i, self.i.size);
        }
        let instr_str_token = SyntaxToken::Text(instr_str.to_string());

        //let mut microcode_str = "   ".to_string();
        let microcode_line_str = match self.trace_instr {
            MC_JUMP => "JMP".to_string(),
            MC_RTN => "RET".to_string(),
            MC_CORR => "COR".to_string(),
            MC_NONE => "   ".to_string(),
            _ => {
                format!("{:03X}", self.trace_instr)
            }
        };
        let microcode_line_token = SyntaxToken::Text(microcode_line_str.to_string());

        let microcode_op_str = match self.trace_instr {
            i if usize::from(i) < MICROCODE_SRC_8088.len() => MICROCODE_SRC_8088[i as usize].to_string(),
            _ => MICROCODE_NUL.to_string(),
        };
        let microcode_op_token = SyntaxToken::Text(microcode_op_str.to_string());

        let _dma_dreq_chr = match self.dma_aen {
            true => 'R',
            false => '.',
        };

        let tx_cycle = match self.is_last_wait() {
            true => 'x',
            false => '.',
        };

        let ready_chr = if self.wait_states > 0 { '.' } else { 'R' };

        let dma_count_str = &format!("{:02} {:02}", dma_count, self.dram_refresh_cycle_num);

        let dma_str = match self.dma_state {
            DmaState::Idle => dma_count_str,
            DmaState::TimerTrigger => "TIMR",
            DmaState::Dreq => "DREQ",
            DmaState::Hrq => "HRQ ",
            DmaState::HoldA => "HLDA",
            DmaState::Operating(n) => match n {
                4 => "S1",
                3 => "S2",
                2 => "S3",
                1 => "S4",
                _ => "S?",
            }, //DmaState::DmaWait(..) => "DMAW"
        };
        let dma_str_token = SyntaxToken::Text(dma_str.to_string());

        let mut comment_str = String::new();
        for c in &self.trace_comment {
            comment_str.push_str(&format!("; {}", c));
        }

        let bus_signal_token = SyntaxToken::Text(format!(
            "M:{}{}{} I:{}{}{}",
            rs_chr, aws_chr, ws_chr, ior_chr, aiow_chr, iow_chr
        ));

        let mut token_vec = vec![
            SyntaxToken::Text(format!("{:04}", self.cycle_num)),
            SyntaxToken::Text(format!("{:04}", self.instr_cycle)),
            ale_token,
            SyntaxToken::Text(format!("{:05X}", self.address_bus)),
            seg_token,
            SyntaxToken::Text(ready_chr.to_string()),
            SyntaxToken::Text(self.wait_states.to_string()),
            SyntaxToken::Text(tx_cycle.to_string()),
            bus_signal_token,
            SyntaxToken::Text(dma_str.to_string()),
            bus_str_token,
            t_str_token,
            SyntaxToken::Text(xfer_str),
            biu_state_new_token,
            SyntaxToken::Text(format!("{:?}", self.fetch_state)),
            q_op_token,
            SyntaxToken::Text(self.last_queue_len.to_string()),
            SyntaxToken::Text(self.queue.to_string()),
            q_read_token,
            microcode_line_token,
            microcode_op_token,
            instr_str_token,
            SyntaxToken::Text(comment_str),
        ];

        token_vec
    }

    pub fn cycle_trace_header(&self) -> Vec<String> {
        vec![
            "Cycle".to_string(),
            "icyc".to_string(),
            "ALE".to_string(),
            "Addr  ".to_string(),
            "Seg".to_string(),
            "Rdy".to_string(),
            "WS".to_string(),
            "Tx".to_string(),
            "8288       ".to_string(),
            "DMA  ".to_string(),
            "Bus ".to_string(),
            "T ".to_string(),
            "Xfer  ".to_string(),
            "BIU".to_string(),
            "Fetch       ".to_string(),
            "Qop".to_string(),
            "Ql".to_string(),
            "Queue   ".to_string(),
            "Qrd   ".to_string(),
            "MCPC".to_string(),
            "Microcode".to_string(),
            "Instr                   ".to_string(),
            "Comments".to_string(),
        ]
    }
}
