use crate::constants;
use crate::context::context;
use crate::context::context::HMContext;
use crate::context::context::Type::{
    AiTrainerRewards, NoContext, NoTokenSplitting, RegularDriver, TokenSplittingFleet,
};
use crate::event::{Event, Type};
use crate::pb::hivemapper::types::v1::{
    AiTrainerPayment, Burn, InitializedAccount, Mint, NoSplitPayment, Output, RegularDriverPayment,
    TokenSplittingPayment, Transfer as Tr,
};
use std::ops::Div;
use substreams::errors::Error;

use substreams_solana::pb::sf::solana::r#type::v1::{
    CompiledInstruction, InnerInstructions, TokenBalance, TransactionStatusMeta,
};

use substreams::log;
use substreams_solana_program_instructions::token_instruction_2022::TokenInstruction;

pub fn process_compiled_instruction(
    output: &mut Output,
    timestamp: i64,
    trx_hash: &String,
    meta: &TransactionStatusMeta,
    inst_index: u32,
    inst: &CompiledInstruction,
    accounts: &Vec<String>,
) {
    let instruction_program_account = &accounts[inst.program_id_index as usize];

    if instruction_program_account == constants::HONEY_TOKEN_INSTRUCTION_PROGRAM {
        match inst.data[0] {
            constants::HONEY_TOKEN_SPLITTING_INSTRUCTION_BYTE => {
                let fleet_account = &accounts[inst.accounts[4] as usize];
                let fleet_driver_account = &accounts[inst.accounts[3] as usize];
                process_inner_instructions(
                    output,
                    timestamp,
                    trx_hash,
                    HMContext {
                        instruction_index: inst_index,
                        r#type: Some(TokenSplittingFleet(context::TokenSplittingFleet {
                            fleet_account: fleet_account.to_owned(),
                            fleet_driver_account: fleet_driver_account.to_owned(),
                        })),
                    },
                    accounts,
                    &meta.inner_instructions,
                    meta,
                );

                return;
            }
            constants::HONEY_REGULAR_DRIVER_INSTRUCTION_BYTE => {
                let driver_account = &accounts[inst.accounts[2] as usize];
                process_inner_instructions(
                    output,
                    timestamp,
                    trx_hash,
                    HMContext {
                        instruction_index: inst_index,
                        r#type: Some(RegularDriver(context::RegularDriver {
                            driver_account: driver_account.to_owned(),
                        })),
                    },
                    accounts,
                    &meta.inner_instructions,
                    meta,
                );

                return;
            }
            constants::HONEY_NO_TOKEN_SPLITTING_INSTRUCTION_BYTE => {
                let driver_account = &accounts[inst.accounts[2] as usize];
                process_inner_instructions(
                    output,
                    timestamp,
                    trx_hash,
                    HMContext {
                        instruction_index: inst_index,
                        r#type: Some(NoTokenSplitting(context::NoTokenSplitting {
                            driver_account: driver_account.to_owned(),
                        })),
                    },
                    accounts,
                    &meta.inner_instructions,
                    meta,
                );

                return;
            }
            constants::HONEY_TOKEN_INSTRUCTION_PROGRAM_MINT => {}
            constants::HONEY_TOKEN_INSTRUCTION_PROGRAM_CREATE_ACCOUNT => {}
            _ => {
                log::info!("instruction program account HONEY_TOKEN_INSTRUCTION_PROGRAM but found no match trx_hash: {} inst.data: {}", trx_hash, inst.data[0]);
            }
        }
    }

    if instruction_program_account == constants::HONEY_TOKEN_INSTRUCTION_PROGRAM_LIB {
        match inst.data[0] {
            constants::HONEY_AI_TRAINER_INSTRUCTION_BYTE => {
                let account = &accounts[inst.accounts[2] as usize];
                process_inner_instructions(
                    output,
                    timestamp,
                    trx_hash,
                    HMContext {
                        instruction_index: inst_index,
                        r#type: Some(AiTrainerRewards(context::AiTrainerRewards {
                            account: account.to_owned(),
                        })),
                    },
                    accounts,
                    &meta.inner_instructions,
                    meta,
                );

                return;
            }
            _ => {
                log::info!("instruction program account HONEY_TOKEN_SPLITTING_CONTRACT but found no match trx_hash: {} inst.data: {}", trx_hash, inst.data[0]);            
            }
        }
    }

    // top level transaction without any inner instructions
    if is_token_program_instruction(accounts, inst.program_id_index as usize) {
        match process_token_instruction(trx_hash, timestamp, &inst.data, &inst.accounts, meta, accounts) {
            Err(err) => {
                panic!(
                    "trx_hash {} top level transaction without inner instructions: {}",
                    trx_hash, err
                );
            }
            Ok(ev_option) => {
                if let Some(ev) = ev_option {
                    match ev.r#type {
                        Type::Mint(mint) => {
                            output.mints.push(mint);
                        }
                        Type::Burn(burn) => {
                            output.burns.push(burn);
                        }
                        Type::Transfer(transfer) => {
                            output.transfers.push(transfer);
                        }
                        Type::InitializeAccount(initialize_account) => {
                            output.initialized_account.push(initialize_account);
                        }
                    }
                }
            }
        }

        return;
    }

    process_inner_instructions(
        output,
        timestamp,
        trx_hash,
        HMContext {
            instruction_index: inst_index,
            r#type: Some(NoContext()),
        },
        accounts,
        &meta.inner_instructions,
        meta,
    );
}

pub fn process_inner_instructions(
    output: &mut Output,
    timestamp: i64,
    trx_hash: &String,
    context: HMContext,
    accounts: &Vec<String>,
    inner_instructions: &Vec<InnerInstructions>,
    meta: &TransactionStatusMeta,
) {
    match context.r#type.as_ref().unwrap() {
        TokenSplittingFleet(obj) => {
            let mut manager_mint = None;
            let mut driver_mint = None;
            inner_instructions
                .iter()
                .filter(|&inner_instruction| inner_instruction.index == context.instruction_index)
                .for_each(|inner_instruction| {
                    inner_instruction
                        .instructions
                        .iter()
                        .filter(|&inst| is_token_program_instruction(accounts, inst.program_id_index as usize))
                        .for_each(|inst| {
                            match process_token_instruction(
                                &trx_hash,
                                timestamp,
                                &inst.data,
                                &inst.accounts,
                                meta,
                                accounts,
                            ) {
                                Err(err) => {
                                    panic!("trx_hash {} token splitting fleet: {}", trx_hash, err);
                                }
                                Ok(ev_option) => {
                                    if let Some(ev) = ev_option {
                                        match ev.r#type {
                                            Type::Mint(mint) => {
                                                if mint.to.eq(&obj.fleet_account) {
                                                    manager_mint = Some(mint);
                                                } else if mint.to.eq(&obj.fleet_driver_account) {
                                                    driver_mint = Some(mint);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        });
                });

            if manager_mint.is_some() && driver_mint.is_some() {
                output.token_splitting_payments.push(TokenSplittingPayment {
                    manager_mint,
                    driver_mint,
                })
            }
        }
        RegularDriver(obj) => {
            process_inner_instructions_mint(
                context.instruction_index,
                &obj.driver_account,
                trx_hash,
                timestamp,
                inner_instructions,
                meta,
                accounts,
            )
            .into_iter()
            .for_each(|mint| {
                output
                    .regular_driver_payments
                    .push(RegularDriverPayment { mint: Some(mint) });
            });
        }
        NoTokenSplitting(obj) => {
            process_inner_instructions_mint(
                context.instruction_index,
                &obj.driver_account,
                trx_hash,
                timestamp,
                inner_instructions,
                meta,
                accounts,
            )
            .into_iter()
            .for_each(|mint| {
                output.ai_trainer_payments.push(AiTrainerPayment { mint: Some(mint) });
            });
        }
        AiTrainerRewards(obj) => {
            process_inner_instructions_mint(
                context.instruction_index,
                &obj.account,
                trx_hash,
                timestamp,
                inner_instructions,
                meta,
                accounts,
            )
            .into_iter()
            .for_each(|mint| {
                output.no_split_payments.push(NoSplitPayment { mint: Some(mint) });
            });
        }
        NoContext() => {
            process_no_context_inner_instructions(
                output,
                context.instruction_index,
                meta,
                &accounts,
                &trx_hash,
                timestamp,
            );
        }
    }
}

pub fn process_no_context_inner_instructions(
    output: &mut Output,
    instruction_index: u32,
    meta: &TransactionStatusMeta,
    accounts: &Vec<String>,
    trx_hash: &String,
    timestamp: i64,
) {
    meta.inner_instructions
        .iter()
        .filter(|inst| inst.index == instruction_index)
        .for_each(|inst| {
            inst.instructions
                .iter()
                .filter(|&inner_instruction| {
                    is_token_program_instruction(accounts, inner_instruction.program_id_index as usize)
                })
                .for_each(|inner_instruction| {
                    match process_token_instruction(
                        trx_hash,
                        timestamp,
                        &inner_instruction.data,
                        &inner_instruction.accounts,
                        meta,
                        accounts,
                    ) {
                        Err(err) => {
                            panic!("trx_hash {} filtering inner instructions: {}", trx_hash, err)
                        }
                        Ok(ev_option) => {
                            if let Some(ev) = ev_option {
                                match ev.r#type {
                                    Type::Mint(mint) => output.mints.push(mint),
                                    Type::Burn(burn) => output.burns.push(burn),
                                    Type::Transfer(transfer) => {
                                        output.transfers.push(transfer);
                                    }
                                    Type::InitializeAccount(initialize_account) => {
                                        output.initialized_account.push(initialize_account);
                                    }
                                }
                            }
                        }
                    }
                })
        });
}

fn process_token_instruction(
    trx_hash: &String,
    timestamp: i64,
    data: &Vec<u8>,
    inst_accounts: &Vec<u8>,
    meta: &TransactionStatusMeta,
    accounts: &Vec<String>,
) -> Result<Option<Event>, Error> {
    match TokenInstruction::unpack(&data) {
        Err(err) => {
            substreams::log::info!("unpacking token instruction {:?}", err);
            return Err(anyhow::anyhow!("unpacking token instruction: {}", err));
        }
        Ok(instruction) => match instruction {
            TokenInstruction::Transfer { amount: amt } => {
                let authority = &accounts[inst_accounts[2] as usize];
                if is_honey_token_transfer(&meta.pre_token_balances, &authority) {
                    let source = &accounts[inst_accounts[0] as usize];
                    let destination = &accounts[inst_accounts[1] as usize];
                    return Ok(Some(Event {
                        r#type: (Type::Transfer(Tr {
                            trx_hash: trx_hash.to_owned(),
                            timestamp,
                            from: source.to_owned(),
                            to: destination.to_owned(),
                            amount: amount_to_decimals(amt as f64, constants::HONEY_TOKEN_DECIMALS as f64),
                        })),
                    }));
                }
            }
            TokenInstruction::TransferChecked { amount: amt, .. } => {
                let mint = &accounts[inst_accounts[1] as usize];
                if is_honey_token(mint) {
                    let source = &accounts[inst_accounts[0] as usize];
                    let destination = &accounts[inst_accounts[2] as usize];
                    return Ok(Some(Event {
                        r#type: (Type::Transfer(Tr {
                            trx_hash: trx_hash.to_owned(),
                            timestamp,
                            from: source.to_owned(),
                            to: destination.to_owned(),
                            amount: amount_to_decimals(amt as f64, constants::HONEY_TOKEN_DECIMALS as f64),
                        })),
                    }));
                }
            }
            TokenInstruction::MintTo { amount: amt } | TokenInstruction::MintToChecked { amount: amt, .. } => {
                let mint = fetch_account_to(&accounts, inst_accounts[0]);
                if mint.ne(&constants::HONEY_CONTRACT_ADDRESS) {
                    return Ok(None);
                }

                let account_to = fetch_account_to(&accounts, inst_accounts[1]);
                return Ok(Some(Event {
                    r#type: (Type::Mint(Mint {
                        trx_hash: trx_hash.to_owned(),
                        timestamp,
                        to: account_to,
                        amount: amount_to_decimals(amt as f64, constants::HONEY_TOKEN_DECIMALS as f64),
                    })),
                }));
            }
            TokenInstruction::Burn { amount: amt } | TokenInstruction::BurnChecked { amount: amt, .. } => {
                let mint = fetch_account_to(&accounts, inst_accounts[1]);
                if mint.ne(&constants::HONEY_CONTRACT_ADDRESS) {
                    return Ok(None);
                }

                let account_from = fetch_account_to(&accounts, inst_accounts[0]);
                return Ok(Some(Event {
                    r#type: (Type::Burn(Burn {
                        trx_hash: trx_hash.to_owned(),
                        timestamp,
                        from: account_from,
                        amount: amount_to_decimals(amt as f64, constants::HONEY_TOKEN_DECIMALS as f64),
                    })),
                }));
            }
            TokenInstruction::InitializeAccount {} => {
                let mint = fetch_account_to(&accounts, inst_accounts[1]);
                if mint.ne(&constants::HONEY_CONTRACT_ADDRESS) {
                    return Ok(None);
                }

                let account = fetch_account_to(&accounts, inst_accounts[0]);
                let owner = fetch_account_to(&accounts, inst_accounts[2]);
                return Ok(Some(Event {
                    r#type: (Type::InitializeAccount(InitializedAccount {
                        trx_hash: trx_hash.to_owned(),
                        account,
                        mint,
                        owner,
                    })),
                }));
            }
            TokenInstruction::InitializeAccount2 { owner: ow } | TokenInstruction::InitializeAccount3 { owner: ow } => {
                let mint = fetch_account_to(&accounts, inst_accounts[1]);
                if mint.ne(&constants::HONEY_CONTRACT_ADDRESS) {
                    return Ok(None);
                }

                let account = fetch_account_to(&accounts, inst_accounts[0]);
                return Ok(Some(Event {
                    r#type: (Type::InitializeAccount(InitializedAccount {
                        trx_hash: trx_hash.to_owned(),
                        account,
                        mint,
                        owner: bs58::encode(ow).into_string(),
                    })),
                }));
            }
            _ => {}
        },
    }

    return Ok(None);
}

fn process_inner_instructions_mint(
    context_instruction_index: u32,
    context_account: &String,
    trx_hash: &String,
    timestamp: i64,
    inner_instructions: &Vec<InnerInstructions>,
    meta: &TransactionStatusMeta,
    accounts: &Vec<String>,
) -> Vec<Mint> {
    let mut mints = vec![];
    inner_instructions
        .iter()
        .filter(|&inner_instruction| inner_instruction.index == context_instruction_index)
        .for_each(|inner_instruction| {
            inner_instruction
                .instructions
                .iter()
                .filter(|&inst| is_token_program_instruction(accounts, inst.program_id_index as usize))
                .for_each(|inst| {
                    match process_token_instruction(&trx_hash, timestamp, &inst.data, &inst.accounts, meta, accounts) {
                        Err(err) => {
                            panic!("trx_hash {} process inner instructions mint: {}", trx_hash, err);
                        }
                        Ok(ev_option) => {
                            if let Some(ev) = ev_option {
                                match ev.r#type {
                                    Type::Mint(mint) => {
                                        if mint.to.eq(context_account) {
                                            mints.push(mint);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                });
        });
    return mints;
}

fn amount_to_decimals(amount: f64, decimal: f64) -> f64 {
    let base: f64 = 10.0;
    return amount.div(&(base.powf(decimal)));
}

fn fetch_account_to(account_keys: &Vec<String>, position: u8) -> String {
    // Instruction account will contain the list of accounts to fetch in the accounts list
    // inst account pos 0 -> mint_info
    // inst account pos 1 -> destination_account_info
    // inst account pos 2 -> owner_info
    return account_keys[position as usize].to_owned();
}

fn is_token_program_instruction(accounts: &Vec<String>, program_id_index: usize) -> bool {
    return &accounts[program_id_index] == constants::TOKEN_PROGRAM;
}

fn is_honey_token(account: &String) -> bool {
    return account.eq(constants::HONEY_CONTRACT_ADDRESS);
}

fn is_honey_token_transfer(pre_token_balances: &Vec<TokenBalance>, account: &String) -> bool {
    for token_balance in pre_token_balances.iter() {
        if token_balance.owner.eq(account) && token_balance.mint.eq(constants::HONEY_CONTRACT_ADDRESS) {
            return true;
        }
    }
    return false;
}

#[cfg(test)]
mod test {
    use crate::utils::amount_to_decimals;

    #[test]
    pub fn test_amount_to_decimals() {
        let amount = 4983184141.0;
        let expected = 4.983184141;

        let actual = amount_to_decimals(amount, 9 as f64);
        println!("expected {:?} actual {:?}", expected, actual);
        assert_eq!(expected, actual)
    }
}
