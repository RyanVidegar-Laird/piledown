#!/usr/bin/env bash

#SBATCH --output=.slurm/%j-makeTestBam.out
#SBATCH  -N 1
#SBATCH  -n 16
#SBATCH  -p general
#SBATCH  --mem=16
#SBATCH  -t 00-01:00:00

set -euo pipefail

INPUT=$1
OUTPUT=$2
PERCKEEP=$3
OUTDIR=$(dirname ${OUTPUT})

TMPBAM=$(mktemp -p ${OUTDIR})

samtools view -@ 16 -h -b -z on -s 846.${PERCKEEP} ${INPUT} -o ${TMPBAM}
samtools sort -@ 16 ${TMPBAM} -o ${OUTPUT}
rm $TMPBAM

samtools index -@ 16 $OUTPUT
