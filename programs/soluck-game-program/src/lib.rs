use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Transfer as SplTransfer};
use borsh::{BorshDeserialize, BorshSerialize};
use std::mem::size_of;
//use switchboard_on_demand::on_demand::accounts::pull_feed::PullFeedAccountData;
declare_id!("8Y9vC2nG3LwfSDqHHdYMMdY4TU5EmMx68JYi2X1ugBSr");

/*
Game Status:
0 - Not Started
1 - In Progress
2 - Ended
*/

#[program]
pub mod soluck_game_program {

    use super::*;
    use anchor_lang::solana_program::{self, program::get_return_data, system_instruction};
    use anchor_spl::token;
    use solana_program::{instruction::Instruction, program::invoke};

    pub fn init_config(ctx: Context<InitConfig>, auth: Pubkey) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;

        if config.is_init == true {
            return Err(GameErrors::ConfigAlreadyInitialized.into());
        }

        config.is_init = true;
        config.game_count = 1;
        config.auth = auth;
        config.whitelisted_tokens = Vec::new();
        config.commission_rate = 5;

        Ok(())
    }

    pub fn add_token_data(
        ctx: Context<UpdateConfigData>,
        token_address: Pubkey,
        oracle_address: Pubkey,
    ) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;
        let signer = ctx.accounts.auth.key;

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        if config.whitelisted_tokens.len() >= 10 {
            return Err(GameErrors::TokenDataListFull.into());
        }

        if config
            .whitelisted_tokens
            .iter()
            .any(|token_data| token_data.token_address == token_address)
        {
            return Err(GameErrors::TokenAlreadyExists.into());
        }

        /* Save the Token to the Whitelist */
        let new_token_data = TokenData {
            token_address,
            oracle_address,
        };

        config.whitelisted_tokens.push(new_token_data);

        Ok(())
    }

    pub fn remove_token_data(ctx: Context<UpdateConfigData>, token_address: Pubkey) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;
        let signer = ctx.accounts.auth.key;

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        /* Remove the Token from the List */
        if let Some(index) = config
            .whitelisted_tokens
            .iter()
            .position(|x| x.token_address == token_address)
        {
            config.whitelisted_tokens.remove(index);
            Ok(())
        } else {
            Err(GameErrors::TokenDataNotFound.into())
        }
    }

    pub fn update_commission_rate(
        ctx: Context<UpdateConfigData>,
        commission_rate: u64,
    ) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;
        let signer = ctx.accounts.auth.key;

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        if commission_rate > 100 {
            return Err(GameErrors::CommissionRateOutOfRange.into());
        } else {
            config.commission_rate = commission_rate;
            Ok(())
        }
    }

    pub fn init_game(ctx: Context<InitGame>, min_limit: u64, max_limit: u64) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;
        let signer = ctx.accounts.auth.key;
        let game = &mut ctx.accounts.game;

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        if game.status != 0 {
            return Err(GameErrors::GameAlreadyInitialized.into());
        }
        
        game.min_limit = min_limit;
        game.max_limit = max_limit;
        game.status = 1;
        game.players = Vec::new();
        game.values = Vec::new();

        config.game_count += 1;

        Ok(())
    }

    pub fn enter_game_sol(ctx: Context<EnterGameSol>, amount: u64, price: u64) -> Result<()> {
        /* Context States and Checks */
        let game = &mut ctx.accounts.game;
        let player = &ctx.accounts.player;

        if game.status != 1 {
            return Err(GameErrors::NotInProgress.into());
        }

        /* TODO: Switchboard integration to be added
        let feed_account = ctx.accounts.feed.data.borrow();
        let feed = PullFeedAccountData::parse(feed_account).unwrap();
        let price = feed.value();
        msg!("price: {:?}", price);*/

        let feed_price: u64 = price;
        let sol_usdc_value = amount * feed_price;

        /* Room Limit Control */
        if sol_usdc_value > game.max_limit || sol_usdc_value < game.min_limit {
            return Err(GameErrors::PriceOutOfRange.into());
        }

        /* SOL Transfer to the Game PDA */
        let transfer_instruction = system_instruction::transfer(&player.key(), &game.key(), amount);
        anchor_lang::solana_program::program::invoke_signed(
            &transfer_instruction,
            &[
                player.to_account_info(),
                game.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[],
        )?;

        /* Save User to the Game PDA */
        game.players.push(player.key());
        game.values.push(sol_usdc_value);

        emit!(EnterGameSolEvent {
            player: player.key(),
            amount: amount,
        });

        Ok(())
    }

    pub fn enter_game_spl(ctx: Context<EnterGameSpl>) -> Result<()> {
        /* Context States and Checks */
        let config = &ctx.accounts.config;
        let game = &mut ctx.accounts.game;
        let source = &ctx.accounts.from_ata;
        let token_mint = source.mint;
        

        if !config
            .whitelisted_tokens
            .iter()
            .any(|token_data| token_data.token_address == token_mint)
        {
            return Err(GameErrors::NotAuth.into());
        }

        if game.status != 1 {
            return Err(GameErrors::NotInProgress.into());
        }

        /* Rest of the Context State */
        let destination = &ctx.accounts.to_ata;
        let token_program = &ctx.accounts.token_program;
        let authority = &ctx.accounts.player;
        let player = &ctx.accounts.player;

        /* SPL Transfer to the Game PDA */
        let cpi_accounts = SplTransfer {
            from: source.to_account_info().clone(),
            to: destination.to_account_info().clone(),
            authority: authority.to_account_info().clone(),
        };
        let cpi_program = token_program.to_account_info();
        let spl_amount = source.amount;

        token::transfer(CpiContext::new(cpi_program, cpi_accounts), spl_amount)?;

        /* Save User to the Game PDA */
        game.players.push(player.key());
        game.values.push(spl_amount);

        emit!(EnterGameSplEvent {
            player: player.key(),
            amount: spl_amount,
            mint: token_mint,
        });

        Ok(())
    }

    pub fn get_random_decide_winner(ctx: Context<GetRandomDecideWinner>) -> Result<()> {
        /* Context States and Checks */
        let config = &mut ctx.accounts.config;
        let signer = ctx.accounts.sender.key;
        let game = &mut ctx.accounts.game;
       

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        if game.status != 1 {
            return Err(GameErrors::NotInProgress.into());
        }

        /* Feed Protocol's instruction calls */
        let rng_program = ctx.accounts.rng_program.key;
        let instruction = Instruction {
            program_id: *rng_program,
            accounts: vec![
                ctx.accounts.sender.to_account_metas(Some(true))[0].clone(),
                ctx.accounts.feed_account_1.to_account_metas(Some(false))[0].clone(),
                ctx.accounts.feed_account_2.to_account_metas(Some(false))[0].clone(),
                ctx.accounts.feed_account_3.to_account_metas(Some(false))[0].clone(),
                ctx.accounts.fallback_account.to_account_metas(Some(false))[0].clone(),
                ctx.accounts
                    .current_feeds_account
                    .to_account_metas(Some(false))[0]
                    .clone(),
                ctx.accounts.temp.to_account_metas(Some(true))[0].clone(),
                ctx.accounts.system_program.to_account_metas(Some(false))[0].clone(),
            ],
            data: vec![0],
        };

        let account_infos = &[
            ctx.accounts.sender.to_account_info().clone(),
            ctx.accounts.feed_account_1.to_account_info().clone(),
            ctx.accounts.feed_account_2.to_account_info().clone(),
            ctx.accounts.feed_account_3.to_account_info().clone(),
            ctx.accounts.fallback_account.to_account_info().clone(),
            ctx.accounts.current_feeds_account.to_account_info().clone(),
            ctx.accounts.temp.to_account_info().clone(),
            ctx.accounts.system_program.to_account_info().clone(),
        ];

        invoke(&instruction, account_infos)?;

        /* Receive the Random Number from Feed */
        let returned_data: (Pubkey, Vec<u8>) = get_return_data().unwrap();

        if &returned_data.0 == rng_program {
            let random_number = RandomNumber::try_from_slice(&returned_data.1)?;
            let game = &mut ctx.accounts.game;

            let players = &game.players;
            let values = &game.values;

            let total_value: u64 = values.iter().sum();

            let adjusted_winning_number = (random_number.random_number % total_value) + 1;

            let mut cumulative_value: u64 = 0;
            for (i, &value) in values.iter().enumerate() {
                cumulative_value += value;

                if adjusted_winning_number < cumulative_value {
                    game.winner = players[i];
                    break;
                }
            }

            game.status = 2;

            emit!(WinnerEvent {
                winner: game.winner,
                game: game.key(),
            });

            Ok(())
        } else {
            return Err(GameErrors::FailedToGetRandomNumber.into());
        }
    }

    pub fn transfer_to_winner(ctx: Context<TransferToWinner>) -> Result<()> {
         /* Context States and Checks */
        let config = &ctx.accounts.config;
        let signer = ctx.accounts.sender.key;
        let game = &ctx.accounts.game;
        let winner = &ctx.accounts.winner;

        if *signer != config.auth {
            return Err(GameErrors::NotAuth.into());
        }

        if game.status != 2 {
            return Err(GameErrors::InProgress.into());
        }

        if winner.key() != game.winner.key() {
            return Err(GameErrors::NotWinner.into());
        }

        let source = &ctx.accounts.from_ata;
        let token_program = &ctx.accounts.token_program;
        let destination = &ctx.accounts.to_ata;
        let commission_rate = ctx.accounts.config.commission_rate;

        // Fee Calculation
        let spl_amount = source.amount;
        let sol_amount: u64 = game.to_account_info().lamports();
        let total_value = game.values.iter().sum::<u64>();
        let sol_usdc_value = total_value - spl_amount;

        let commission = total_value * commission_rate / 100;

        let spl_amount_after_commission =
            spl_amount.saturating_sub(commission * spl_amount / total_value);
        let sol_amount_after_commission =
            sol_amount.saturating_sub(commission * sol_usdc_value / total_value);

        // Transfer the SPL tokens to the winner
        let cpi_accounts = SplTransfer {
            from: source.to_account_info().clone(),
            to: destination.to_account_info().clone(),
            authority: game.to_account_info().clone(),
        };
        let cpi_program = token_program.to_account_info();

        token::transfer(
            CpiContext::new(cpi_program, cpi_accounts),
            spl_amount_after_commission,
        )?;

        // Transfer the SOL  to the winner
        let transfer_instruction =
            system_instruction::transfer(&game.key(), &winner.key(), sol_amount_after_commission);
        anchor_lang::solana_program::program::invoke_signed(
            &transfer_instruction,
            &[
                game.to_account_info(),
                winner.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[],
        )?;

        Ok(())
    }

    pub fn withdraw_from_pda(ctx: Context<TransferToWinner>) -> Result<()>{
        //TODO: Implement
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct RandomNumber {
    pub random_number: u64,
}

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(
        init,
        seeds = [b"config"],
        bump,
        payer = signer,
        space = 8 + size_of::<ConfigData>()+ size_of::<TokenData>() * 10 + size_of::<Pubkey>() * 5,
    )]
    pub config: Account<'info, ConfigData>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct ConfigData {
    pub is_init: bool,
    pub game_count: u64,
    pub auth: Pubkey,
    pub whitelisted_tokens: Vec<TokenData>,
    pub commission_rate: u64,
}

#[account]
pub struct TokenData {
    pub token_address: Pubkey,
    pub oracle_address: Pubkey,
}

#[derive(Accounts)]
pub struct UpdateConfigData<'info> {
    #[account(mut)]
    pub config: Account<'info, ConfigData>,
    #[account(signer)]
    /// CHECK:
    pub auth: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct InitGame<'info> {
    #[account(mut)]
    pub config: Account<'info, ConfigData>,

    #[account(
        init,
        seeds = [b"game", config.game_count.to_string().as_bytes()],
        bump,
        payer = auth,
        space = 170,
    )]
    pub game: Account<'info, GameData>,

    #[account(mut)]
    pub auth: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct GameData {
    pub status: u8,
    pub players: Vec<Pubkey>,
    pub values: Vec<u64>,
    pub max_limit: u64,
    pub min_limit: u64,
    pub winner: Pubkey,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct EnterGameSol<'info> {
    #[account(mut)]
    pub config: Account<'info, ConfigData>,
    #[account(mut)]
    pub game: Account<'info, GameData>,

    #[account(mut)]
    pub player: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EnterGameSpl<'info> {
    #[account(mut)]
    pub config: Account<'info, ConfigData>,
    #[account(mut)]
    pub game: Account<'info, GameData>,

    #[account(mut)]
    pub player: Signer<'info>,

    #[account(mut)]
    pub from_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to_ata: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GetRandomDecideWinner<'info> {
    #[account(mut)]
    pub config: Account<'info, ConfigData>,
    #[account(mut)]
    pub game: Account<'info, GameData>,

    #[account(mut)]
    pub sender: Signer<'info>,

    /// CHECK: Feed Protocol's on-chain random provider accounts
    pub feed_account_1: AccountInfo<'info>,
    /// CHECK:
    pub feed_account_2: AccountInfo<'info>,
    /// CHECK:
    pub feed_account_3: AccountInfo<'info>,
    /// CHECK:
    pub fallback_account: AccountInfo<'info>,
    #[account(mut)]
    /// CHECK:
    pub current_feeds_account: AccountInfo<'info>,
    /// CHECK:
    pub rng_program: AccountInfo<'info>,
    #[account(mut)]
    /// CHECK:
    pub temp: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferToWinner<'info> {
    #[account(mut)]
    pub game: Account<'info, GameData>,
    #[account(mut)]
    pub config: Account<'info, ConfigData>,

    /// CHECK:
    #[account(mut)]
    pub winner: AccountInfo<'info>,

    #[account(mut)]
    pub from_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to_ata: Account<'info, TokenAccount>,

    #[account(mut)]
    pub sender: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// Events
#[event]
pub struct EnterGameSolEvent {
    player: Pubkey,
    amount: u64,
}

#[event]
pub struct EnterGameSplEvent {
    player: Pubkey,
    amount: u64,
    mint: Pubkey,
}

#[event]
pub struct WinnerEvent {
    winner: Pubkey,
    game: Pubkey,
}

// Errors
#[error_code]
pub enum GameErrors {
    #[msg("Config already initialized")]
    ConfigAlreadyInitialized,
    #[msg("Not an auth")]
    NotAuth,
    #[msg("Game in progress")]
    InProgress,
    #[msg("Game not in progress")]
    NotInProgress,
    #[msg("Not the winner")]
    NotWinner,
    #[msg("Failed to get random number")]
    FailedToGetRandomNumber,
    #[msg("Address not found")]
    AddressNotFound,
    #[msg("Already ended")]
    AlreadyEnded,
    #[msg("Game already initialized")]
    GameAlreadyInitialized,
    #[msg("The token data list is full.")]
    TokenDataListFull,
    #[msg("The token data was not found.")]
    TokenDataNotFound,
    #[msg("Entry Price out of range")]
    PriceOutOfRange,
    #[msg("Token already exists")]
    TokenAlreadyExists,
    #[msg("Commission rate out of range")]
    CommissionRateOutOfRange,
}
